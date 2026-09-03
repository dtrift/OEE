//! The week-5 main experiment (D4-D6, plan section 10): the full
//! simulator -> nodes -> MQTT -> aggregator loop, per scenario, measured
//! vs true OEE, plus the determinism check (one seed run twice -> an
//! identical result).
//!
//! Artifacts land in `tmp/experiment/` (gitignored): the node offline CSVs
//! and the aggregator windows CSV per scenario — the D4 "raw runs". The
//! table prints with `--nocapture` and is copied into the week-5 gate doc.
//!
//! Truth (ground truth by construction):
//! - A = run intervals / duration (scenario events, machine starts idle);
//! - P = ideal_cycle * belt_parts / run (the nominal ideal: 400 ms);
//! - Q = good taps / total taps (1.0 without taps);
//! - OEE = A x P x Q — the product never sees a model, only the scenario.

use std::fs;
use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use line_simulator::scenario::{Scenario, SAMPLE_RATE_HZ};
use line_simulator::{belt, taps, Simulator};
use mqtt_min::testing::LoopbackBroker;
use nodes::mqtt_sink::MqttSink;
use nodes::sim_source::{IrSource, SimSource, TapSource};
use nodes::status::{CsvStatusLog, MultiSink, VecSink};
use oee_aggregator::aggregator::{self, Config};
use oee_aggregator::windows::WindowStats;

/// The nominal ideal cycle of the line, ms — a line property; the slowdown
/// scenario slows the *belt*, the ideal stays.
const IDEAL_CYCLE_MS: u32 = 400;

/// The experiment artifacts directory (gitignored `tmp/`).
const ARTIFACTS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../tmp/experiment");

/// One row of the measured-vs-truth table.
struct Row {
    scenario: &'static str,
    seed: u64,
    truth: Components,
    measured: Components,
    measured_parts: u32,
}

/// The three components + the product.
#[derive(Debug, Clone, Copy)]
struct Components {
    a: f32,
    p: f32,
    q: f32,
    oee: f32,
}

/// Runs the simulator's current channel into the run-CSV text (the same
/// rows the CLI writes via `--out`).
fn run_csv(scenario: &Scenario, seed: u64) -> String {
    let mut simulator = Simulator::new(seed, scenario.signal);
    let total = (scenario.duration_ms as u64 * SAMPLE_RATE_HZ as u64) / 1000;
    let mut next_event = 0usize;
    let mut text = String::from("t_ms,current_a,state\n");
    for _ in 0..total {
        while next_event < scenario.events.len()
            && scenario.events[next_event].t_ms <= simulator.next_t_ms()
        {
            simulator.apply(scenario.events[next_event].state);
            next_event += 1;
        }
        let sample = simulator.next_sample(&scenario.envelope, &scenario.noise);
        text.push_str(&format!(
            "{},{:.4},{}\n",
            sample.t_ms,
            sample.current_a,
            sample.state.as_str()
        ));
    }
    text
}

/// The truth of a scenario (ground truth by construction).
fn truth(scenario: &Scenario, seed: u64) -> Components {
    // A: run intervals over the duration (the machine starts idle).
    let mut run_ms = 0u32;
    let mut from = 0u32;
    let mut running = false;
    for event in &scenario.events {
        if running {
            run_ms += event.t_ms - from;
        }
        from = event.t_ms;
        running = event.state == line_simulator::fsm::MachineState::Run;
    }
    if running {
        run_ms += scenario.duration_ms - from;
    }
    let availability = run_ms as f32 / scenario.duration_ms as f32;
    // P: the belt parts under the nominal ideal.
    let parts = belt::generate(scenario, seed).len() as u32;
    let performance = if run_ms > 0 {
        (IDEAL_CYCLE_MS as f32 * parts as f32 / run_ms as f32).min(1.0)
    } else {
        0.0
    };
    // Q: the tap verdicts (independent channel, same nominal cadence).
    let [good, cracked] = taps::verdict_histogram(&taps::generate(scenario, seed));
    let total = good + cracked;
    let quality = if total > 0 {
        good as f32 / total as f32
    } else {
        1.0
    };
    Components {
        a: availability,
        p: performance,
        q: quality,
        oee: availability * performance * quality,
    }
}

/// One full bench run in-process: broker + aggregator + the three nodes
/// over the simulator streams. Returns the measured final shift row and
/// writes the artifacts (node offline CSVs + the aggregator windows CSV).
fn run_bench(scenario_text: &str, name: &str, seed: u64) -> WindowStats {
    let scenario = Scenario::parse(scenario_text).expect("scenario parses");
    fs::create_dir_all(ARTIFACTS).expect("artifacts dir");
    let run_id = format!("{name}-{seed}");

    // The simulator streams (in-memory; also written out for the record).
    let current_csv = run_csv(&scenario, seed);
    let belt_parts = belt::generate(&scenario, seed);
    let mut belt_events = Vec::new();
    belt::write_events_csv(&belt_parts, &scenario.belt, &mut belt_events).unwrap();
    let belt_events = String::from_utf8(belt_events).unwrap();
    let tap_events = taps::generate(&scenario, seed);
    let mut tap_dataset = Vec::new();
    taps::write_dataset_csv(&tap_events, &mut tap_dataset).unwrap();
    let mut tap_meta = Vec::new();
    taps::write_meta_csv(&tap_events, &mut tap_meta).unwrap();
    let tap_dataset = String::from_utf8(tap_dataset).unwrap();
    let tap_meta = String::from_utf8(tap_meta).unwrap();

    let artifact = |suffix: &str| -> String {
        fs::read_to_string(format!("{ARTIFACTS}/{name}_{suffix}")).unwrap_or_default()
    };
    let _ = artifact;

    // Write the raw-run artifacts for the record (D4).
    let write = |suffix: &str, text: &str| {
        fs::File::create(format!("{ARTIFACTS}/{name}_{suffix}"))
            .and_then(|mut file| file.write_all(text.as_bytes()))
            .expect("artifact write");
    };
    write("run.csv", &current_csv);
    write("belt_events.csv", &belt_events);
    write("taps_dataset.csv", &tap_dataset);
    write("taps_meta.csv", &tap_meta);

    // The bench: broker first, then the aggregator (waits for its ready
    // signal), then the nodes.
    let broker = LoopbackBroker::spawn();
    let (ready_tx, ready_rx) = mpsc::channel();
    let windows_csv = format!("{ARTIFACTS}/{name}_oee_windows.csv");
    let config = Config {
        broker_addr: broker.addr.clone(),
        ideal_cycle_ms: IDEAL_CYCLE_MS,
        minute_ms: 60_000,
        expect_nodes: vec!["a".into(), "p".into(), "q".into()],
        csv_path: Some(windows_csv.clone().into()),
        ready: Some(ready_tx),
        ..Config::default()
    };
    let aggregator_thread = thread::spawn(move || aggregator::run(&config));
    ready_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("aggregator subscribed");

    let addr = broker.addr.clone();
    let run_id_a = run_id.clone();
    let node_a = thread::spawn(move || {
        let mut source = SimSource::new(current_csv.as_bytes());
        let mut log = CsvStatusLog::new(Vec::new()).expect("status log");
        let mut mqtt = MqttSink::new(&addr, "node-a");
        mqtt.publish_a_meta("model_a.tflite", nodes::a::WINDOW, 1600);
        let summary = nodes::a::run_a(&mut source, &run_id_a, &mut MultiSink(&mut log, &mut mqtt));
        mqtt.publish_end("a", source.last_t_ms(), &run_id_a);
        (
            summary,
            String::from_utf8(log.into_inner().unwrap()).unwrap(),
        )
    });
    let addr = broker.addr.clone();
    let run_id_p = run_id.clone();
    let node_p = thread::spawn(move || {
        let mut source = IrSource::new(belt_events.as_bytes());
        let mut log = CsvStatusLog::new(Vec::new()).expect("count log");
        let mut mqtt = MqttSink::new(&addr, "node-p");
        mqtt.publish_p_meta();
        let summary = nodes::p::run_p(&mut source, &run_id_p, &mut MultiSink(&mut log, &mut mqtt));
        mqtt.publish_end("p", source.last_t_ms(), &run_id_p);
        (
            summary,
            String::from_utf8(log.into_inner().unwrap()).unwrap(),
        )
    });
    let addr = broker.addr.clone();
    let run_id_q = run_id.clone();
    let node_q = thread::spawn(move || {
        let mut source = TapSource::new(tap_dataset.as_bytes(), tap_meta.as_bytes());
        let mut log = CsvStatusLog::new(Vec::new()).expect("verdict log");
        let mut mqtt = MqttSink::new(&addr, "node-q");
        mqtt.publish_q_meta("model_q.tflite", nodes::q::WINDOW, 16_000);
        let summary = nodes::q::run_q(&mut source, &run_id_q, &mut MultiSink(&mut log, &mut mqtt));
        mqtt.publish_end("q", source.last_t_ms(), &run_id_q);
        (
            summary,
            String::from_utf8(log.into_inner().unwrap()).unwrap(),
        )
    });

    let (summary_a, statuses_csv) = node_a.join().expect("node a thread");
    let (summary_p, counts_csv) = node_p.join().expect("node p thread");
    let (summary_q, verdicts_csv) = node_q.join().expect("node q thread");
    write("statuses.csv", &statuses_csv);
    write("counts.csv", &counts_csv);
    write("verdicts.csv", &verdicts_csv);

    let summary = aggregator_thread
        .join()
        .expect("aggregator thread")
        .expect("aggregator run");
    assert_eq!(summary.parse_errors, 0, "no payload may fail to parse");
    assert!(summary.messages > 10, "a real run has real traffic");
    // The D1 gate, live: the count the aggregator saw equals the belt truth.
    let shift = summary.final_shift.expect("the final shift row");
    assert_eq!(
        shift.parts,
        belt_parts.len() as u32,
        "node P count = belt truth (summary p: {:?})",
        summary_p
    );
    let _ = (summary_a, summary_q); // diagnostics only
    shift
}

/// The scenarios of the experiment (plan section 10).
const SCENARIOS: [(&str, &str); 4] = [
    ("normal", include_str!("../../scenarios/week5/normal.toml")),
    (
        "downtime",
        include_str!("../../scenarios/week5/downtime.toml"),
    ),
    (
        "slowdown",
        include_str!("../../scenarios/week5/slowdown.toml"),
    ),
    (
        "rejects",
        include_str!("../../scenarios/week5/rejects.toml"),
    ),
];

fn components(stats: &WindowStats) -> Components {
    Components {
        a: stats.availability,
        p: stats.performance,
        q: stats.quality,
        oee: stats.oee,
    }
}

fn error(truth: f32, measured: f32) -> f32 {
    measured - truth
}

/// Prints the D4 table (also the gate-doc material).
fn print_table(rows: &[Row]) {
    println!();
    println!("| scenario | seed | true OEE | measured | err | true A | meas A | err | true P | meas P | err | true Q | meas Q | err | parts |");
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    for row in rows {
        let t = &row.truth;
        let m = &row.measured;
        println!(
            "| {} | {} | {:.3} | {:.3} | {:+.3} | {:.3} | {:.3} | {:+.3} | {:.3} | {:.3} | {:+.3} | {:.3} | {:.3} | {:+.3} | {} |",
            row.scenario,
            row.seed,
            t.oee,
            m.oee,
            error(t.oee, m.oee),
            t.a,
            m.a,
            error(t.a, m.a),
            t.p,
            m.p,
            error(t.p, m.p),
            t.q,
            m.q,
            error(t.q, m.q),
            row.measured_parts,
        );
    }
}

#[test]
fn measured_vs_true_oee_across_scenarios() {
    let mut rows = Vec::new();
    for (name, text) in SCENARIOS {
        let scenario = Scenario::parse(text).expect("scenario");
        let truth = truth(&scenario, 42);
        let measured = run_bench(text, name, 42);
        rows.push(Row {
            scenario: name,
            seed: 42,
            truth,
            measured: components(&measured),
            measured_parts: measured.parts,
        });
    }

    print_table(&rows);

    for row in &rows {
        // Loose sanity bounds: the pipeline is honest, not perfect — A's
        // boundary lag and classification noise, P/Q through the models.
        // The bounds catch wiring bugs, not statistics.
        assert!(
            (row.measured.oee - row.truth.oee).abs() < 0.05,
            "{}: OEE error too large: true {:.3} vs measured {:.3}",
            row.scenario,
            row.truth.oee,
            row.measured.oee
        );
        for (name, truth, measured) in [
            ("A", row.truth.a, row.measured.a),
            ("P", row.truth.p, row.measured.p),
            ("Q", row.truth.q, row.measured.q),
        ] {
            assert!(
                (measured - truth).abs() < 0.08,
                "{}: {} error too large: true {:.3} vs measured {:.3}",
                row.scenario,
                name,
                truth,
                measured
            );
        }
    }

    // The slowdown must show up in P (the table is not flat).
    let normal = &rows[0];
    let slowdown = &rows[2];
    assert!(
        slowdown.truth.p < normal.truth.p - 0.1,
        "the slowdown scenario must lower P ({} vs {})",
        slowdown.truth.p,
        normal.truth.p
    );
    assert!(
        slowdown.measured.p < normal.measured.p - 0.08,
        "the measured slowdown must be visible too ({} vs {})",
        slowdown.measured.p,
        normal.measured.p
    );
    // The rejects scenario must show up in Q.
    let rejects = &rows[3];
    assert!(
        rejects.measured.q < normal.measured.q - 0.2,
        "the rejects scenario must lower Q ({} vs {})",
        rejects.measured.q,
        normal.measured.q
    );
    // The downtime scenario must show up in A.
    let downtime = &rows[1];
    assert!(
        downtime.measured.a < normal.measured.a - 0.2,
        "the downtime scenario must lower A ({} vs {})",
        downtime.measured.a,
        normal.measured.a
    );
}

#[test]
fn same_seed_produces_an_identical_result() {
    // Determinism (plan section 10): the whole bench run twice with one
    // seed -> identical aggregator CSV bytes and identical final rows.
    let (name, text) = SCENARIOS[0];
    let first = run_bench(text, &format!("{name}_det1"), 42);
    let second = run_bench(text, &format!("{name}_det2"), 42);
    assert_eq!(first, second, "the final shift rows must be identical");
    let csv_a = fs::read_to_string(format!("{ARTIFACTS}/{name}_det1_oee_windows.csv")).unwrap();
    let csv_b = fs::read_to_string(format!("{ARTIFACTS}/{name}_det2_oee_windows.csv")).unwrap();
    let columns_excluding_run_id = |text: &str| {
        text.lines()
            .map(|line| {
                let fields: Vec<&str> = line.split(',').collect();
                // Drop the run_id column (index 1): it embeds the artifact
                // name, which differs between the two runs by design.
                let mut kept = fields.clone();
                kept.remove(1);
                kept.join(",")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        columns_excluding_run_id(&csv_a),
        columns_excluding_run_id(&csv_b),
        "the windows CSV must be identical (run_id aside)"
    );
    assert!(csv_a.lines().count() >= 3, "header + minute + shift rows");
}

#[test]
fn other_seeds_stay_within_bounds() {
    // D6 robustness: other seeds — errors within reasonable bounds, the
    // conclusions (which component each scenario hits) do not flip.
    for seed in [7u64, 2026] {
        let (_, text) = SCENARIOS[0];
        let scenario = Scenario::parse(text).expect("scenario");
        let truth = truth(&scenario, seed);
        let measured = run_bench(text, &format!("normal_s{seed}"), seed);
        let measured = components(&measured);
        println!(
            "seed {seed}: true OEE {:.3}, measured {:.3} (A {:+.3}, P {:+.3}, Q {:+.3})",
            truth.oee,
            measured.oee,
            measured.a - truth.a,
            measured.p - truth.p,
            measured.q - truth.q
        );
        assert!(
            (measured.oee - truth.oee).abs() < 0.06,
            "seed {seed}: OEE error too large"
        );
    }
}

/// The sensitivity scenario: the normal pattern plus a 150 ms jam blip —
/// around/below node A's temporal resolution (2 windows + hysteresis
/// ~160-240 ms) — so the hysteresis sweep has something real to catch or
/// miss. The `[noise]` sigma is patched in per sweep point.
fn sensitivity_scenario(noise_sigma: f32) -> String {
    format!(
        r#"duration_ms = 60000

[[events]]
t_ms = 2000
state = "Run"

[[events]]
t_ms = 20000
state = "Jam"

[[events]]
t_ms = 20400
state = "Run"

[[events]]
t_ms = 30000
state = "Jam"

[[events]]
t_ms = 30150
state = "Run"

[[events]]
t_ms = 40000
state = "Overload"

[[events]]
t_ms = 40800
state = "Run"

[[events]]
t_ms = 58000
state = "Idle"

[envelope]
idle = 0.4
run = 2.0
jam = 3.2
overload = 4.5

[noise]
sigma_a = {noise_sigma}

[taps]
period_ms = 400
crack_probability = 0.25

[belt]
period_ms = 400
jitter = 0.15
double_probability = 0.1
skip_probability = 0.05
"#
    )
}

/// Machine run time per the scenario events (the A truth).
fn true_run_ms(scenario: &Scenario) -> u32 {
    let mut run_ms = 0u32;
    let mut from = 0u32;
    let mut running = false;
    for event in &scenario.events {
        if running {
            run_ms += event.t_ms - from;
        }
        from = event.t_ms;
        running = event.state == line_simulator::fsm::MachineState::Run;
    }
    if running {
        run_ms += scenario.duration_ms - from;
    }
    run_ms
}

#[test]
fn sensitivity_analysis() {
    // D5: where does the measured OEE lose accuracy? Three sweeps on one
    // scenario shape: simulator noise (A's channel), the A hysteresis
    // depth, the P anti-double threshold. The first runs the full bench;
    // the other two exercise the node directly (the aggregator is already
    // covered by the baseline test above).
    let seed = 42u64;

    // --- Sweep 1: simulator noise (the current-signal channel).
    println!();
    println!("## sensitivity: simulator noise (sigma_a) — full bench");
    println!("| sigma_a | true A | meas A | err A | true OEE | meas OEE | err OEE |");
    println!("| --- | --- | --- | --- | --- | --- | --- |");
    for sigma in [0.12f32, 0.25, 0.40, 0.60] {
        let text = sensitivity_scenario(sigma);
        let scenario = Scenario::parse(&text).expect("scenario");
        let truth = truth(&scenario, seed);
        let measured = run_bench(&text, &format!("sens_noise_{sigma:.2}"), seed);
        let measured = components(&measured);
        println!(
            "| {:.2} | {:.3} | {:.3} | {:+.3} | {:.3} | {:.3} | {:+.3} |",
            sigma,
            truth.a,
            measured.a,
            measured.a - truth.a,
            truth.oee,
            measured.oee,
            measured.oee - truth.oee
        );
        // Even at 5x the training noise the pipeline must stay coherent
        // (bounded error, not garbage).
        assert!((measured.oee - truth.oee).abs() < 0.15, "sigma {sigma}");
    }

    // --- Sweep 2: the A hysteresis depth (node A directly).
    println!();
    println!("## sensitivity: A hysteresis (confirm_after windows)");
    println!("| confirm_after | true run ms | meas run ms | err ms | note |");
    println!("| --- | --- | --- | --- | --- |");
    let text = sensitivity_scenario(0.12);
    let scenario = Scenario::parse(&text).expect("scenario");
    let run_true = true_run_ms(&scenario);
    let csv = run_csv(&scenario, seed);
    for confirm_after in [1u32, 2, 3, 4] {
        let mut source = SimSource::new(csv.as_bytes());
        let mut sink = VecSink::default();
        let summary = nodes::a::run_a_confirmed(&mut source, "sens", &mut sink, confirm_after);
        // Integrate the statuses the way the aggregator does (the step
        // function extends the last status to the stream end).
        let mut run_ms = 0u32;
        let mut from = 0u32;
        let mut running = false;
        for row in &sink.0 {
            if running {
                run_ms += row.t_ms - from;
            }
            from = row.t_ms;
            running = row.state == "run";
        }
        if running {
            run_ms += source.last_t_ms() - from;
        }
        let note = if confirm_after == 2 {
            "line default"
        } else {
            ""
        };
        println!(
            "| {confirm_after} | {run_true} | {run_ms} | {:+} | {note} |",
            run_ms as i64 - run_true as i64
        );
        let _ = summary;
        // Any depth must stay within a few hundred ms of the truth (the
        // boundary lag + the blip at most).
        assert!((run_ms as i64 - run_true as i64).abs() < 500);
    }

    // --- Sweep 3: the P anti-double threshold (node P directly).
    println!();
    println!("## sensitivity: P anti-double window (ms)");
    println!("| window ms | parts | truth | merged | note |");
    println!("| --- | --- | --- | --- | --- |");
    let belt_parts = belt::generate(&scenario, seed);
    let doubles = belt_parts.iter().filter(|p| p.pulses == 2).count();
    let mut events = Vec::new();
    belt::write_events_csv(&belt_parts, &scenario.belt, &mut events).unwrap();
    let events = String::from_utf8(events).unwrap();
    for window_ms in [50u32, 80, 100, 200, 300] {
        let mut source = IrSource::new(events.as_bytes());
        let mut counter = nodes::p::EdgeCounter::new(window_ms);
        while let Ok((t_ms, level)) = source.next_event() {
            counter.on_level(t_ms, level);
        }
        let note = match window_ms {
            50 => "too narrow: doubles re-counted",
            100 => "line default",
            300 => "too wide: real parts merged",
            _ => "",
        };
        println!(
            "| {window_ms} | {} | {} | {} | {note} |",
            counter.parts(),
            belt_parts.len(),
            counter.merged()
        );
        if window_ms == 50 {
            // Below the double-pulse span (pulse + gap = 70 ms) the second
            // pulse counts again: parts = truth + doubles.
            assert_eq!(
                counter.parts() as usize,
                belt_parts.len() + doubles,
                "a too-narrow window must double-count"
            );
        } else if window_ms == 300 {
            // Above the shortest real interval (period*(1-jitter) minus the
            // neighbour's opposite jitter) pairs of real parts merge.
            assert!(
                (counter.parts() as usize) < belt_parts.len(),
                "a too-wide window must undercount"
            );
        } else {
            // Within [double span, min part interval] the count is exact.
            assert_eq!(counter.parts() as usize, belt_parts.len());
        }
    }
}

#[test]
fn q_sensitivity_to_tap_noise() {
    // D5, the Q channel: tap noise vs the model's verdicts (node Q
    // directly over the tap dataset — the aggregator path is unchanged).
    println!();
    println!("## sensitivity: tap noise (noise_sigma) — node Q verdicts");
    println!("| noise_sigma | taps | accuracy | true Q | meas Q |");
    println!("| --- | --- | --- | --- | --- |");
    let seed = 42u64;
    for noise in [0.01f32, 0.04, 0.08, 0.12] {
        let text = format!(
            "duration_ms = 60000\n{}\n[taps]\nperiod_ms = 400\ncrack_probability = 0.25\nnoise_sigma = {noise}\ncrack_noise_boost = 4.0\n",
            r#"[[events]]
t_ms = 1000
state = "Run"

[[events]]
t_ms = 59000
state = "Idle""#
        );
        let scenario = Scenario::parse(&text).expect("scenario");
        let tap_events = taps::generate(&scenario, seed);
        let mut dataset = Vec::new();
        taps::write_dataset_csv(&tap_events, &mut dataset).unwrap();
        let mut meta = Vec::new();
        taps::write_meta_csv(&tap_events, &mut meta).unwrap();
        let mut source = TapSource::new(dataset.as_slice(), meta.as_slice());
        let mut sink = VecSink::default();
        nodes::q::run_q(&mut source, "sens", &mut sink);
        let correct = tap_events
            .iter()
            .zip(&sink.0)
            .filter(|(event, row)| event.verdict.as_str() == row.state)
            .count();
        let accuracy = correct as f32 / tap_events.len() as f32;
        let [good, cracked] = taps::verdict_histogram(&tap_events);
        let q_true = good as f32 / (good + cracked) as f32;
        let good_measured = sink.0.iter().filter(|r| r.state == "good").count() as f32;
        let q_meas = good_measured / sink.0.len() as f32;
        println!(
            "| {noise:.2} | {} | {:.3} | {:.3} | {:.3} |",
            tap_events.len(),
            accuracy,
            q_true,
            q_meas
        );
        // Accuracy may degrade gracefully but must not collapse.
        assert!(accuracy > 0.7, "noise {noise}: accuracy {accuracy}");
    }
}
