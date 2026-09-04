//! Week-4 integration tests (D1/D4/D5):
//! - node A statuses vs the scenario ground truth (mismatches only at
//!   window boundaries);
//! - node Q verdicts vs the tap ground truth;
//! - one coherent run of both nodes with MQTT through the loopback broker;
//! - error isolation: a corrupt window is dropped, the node survives;
//! - microflow-vs-interp parity on the Q model;
//! - a predict-latency smoke ("not worse than the line tempo").

use line_simulator::scenario::Scenario;
use line_simulator::{belt, taps, Simulator};
use mqtt_min::testing::LoopbackBroker;
use nalgebra::SMatrix;
use nodes::mqtt_sink::MqttSink;
use nodes::sim_source::{IrSource, SimSource, TapSource};
use nodes::status::{MultiSink, StatusRow, VecSink};
use nodes::{a, p, q};

const BASE_TOML: &str = include_str!("../../scenarios/base.toml");
const TAPS_TOML: &str = include_str!("../../scenarios/taps.toml");

/// A belt-heavy scenario text: doubles and skips tuned up (the D1 gate
/// check runs against exactly this shape).
const BELT_TOML: &str = r#"
duration_ms = 30000

[[events]]
t_ms = 1000
state = "Run"

[[events]]
t_ms = 29000
state = "Idle"

[belt]
period_ms = 400
jitter = 0.15
double_probability = 0.3
skip_probability = 0.2
pulse_ms = 30
double_gap_ms = 40
"#;

/// Runs the simulator over a scenario, returning the run CSV text.
fn run_csv(scenario: &Scenario, seed: u64) -> String {
    let mut simulator = Simulator::new(seed, scenario.signal);
    let total =
        (scenario.duration_ms as u64 * line_simulator::scenario::SAMPLE_RATE_HZ as u64) / 1000;
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

/// The taps dataset + meta CSV texts.
fn taps_csv(scenario: &Scenario, seed: u64) -> (String, String) {
    let events = taps::generate(scenario, seed);
    let mut dataset = Vec::new();
    taps::write_dataset_csv(&events, &mut dataset).unwrap();
    let mut meta = Vec::new();
    taps::write_meta_csv(&events, &mut meta).unwrap();
    (
        String::from_utf8(dataset).unwrap(),
        String::from_utf8(meta).unwrap(),
    )
}

/// Truth intervals from scenario events: (from_ms, to_ms, state).
fn truth_intervals(scenario: &Scenario) -> Vec<(u32, u32, String)> {
    let mut intervals = Vec::new();
    let mut from = 0u32;
    let mut state = "idle".to_string();
    for event in &scenario.events {
        intervals.push((from, event.t_ms, state.clone()));
        from = event.t_ms;
        state = event.state.as_str().to_string();
    }
    intervals.push((from, scenario.duration_ms, state));
    intervals
}

/// The node status valid at time t (last status row with t_ms <= t).
fn status_at(statuses: &[StatusRow], t_ms: u32) -> Option<&str> {
    statuses
        .iter()
        .filter(|row| row.t_ms <= t_ms)
        .map(|row| row.state.as_str())
        .next_back()
}

#[test]
fn node_a_statuses_match_truth_away_from_boundaries() {
    let scenario = Scenario::parse(BASE_TOML).expect("base scenario");
    let csv = run_csv(&scenario, 42);
    let mut source = SimSource::new(csv.as_bytes());
    let mut sink = VecSink::default();
    let summary = a::run_a(&mut source, "run1", &mut sink);

    assert!(summary.windows > 700, "60 s at 1.6 kHz -> ~740 windows");
    assert_eq!(
        summary.dirty_windows, 0,
        "a clean stream has no dirty windows"
    );

    // The node must report every scenario state at least once.
    let states: Vec<&str> = sink.0.iter().map(|row| row.state.as_str()).collect();
    for expected in ["idle", "run", "jam", "overload"] {
        assert!(
            states.contains(&expected),
            "status {expected} never emitted: {states:?}"
        );
    }

    // Deep checkpoints: 1 s inside every truth interval the status must be
    // exact (boundaries may lag by the window + hysteresis, ~240 ms).
    for (from, to, state) in truth_intervals(&scenario) {
        if to - from < 2000 {
            continue; // too short to probe deep inside
        }
        let probe = from + 1000;
        let got = status_at(&sink.0, probe).expect("a status exists by then");
        assert_eq!(
            got, state,
            "at t={probe} ms (interval {from}..{to} = {state}) the node says {got}"
        );
    }
}

#[test]
fn node_a_survives_a_corrupt_window() {
    let scenario = Scenario::parse(BASE_TOML).expect("base scenario");
    let csv = run_csv(&scenario, 42);
    // Corrupt ~1 window in the middle: 128 bad rows around the middle.
    let lines: Vec<&str> = csv.lines().collect();
    let mut broken = String::new();
    let mid = lines.len() / 2;
    for (i, line) in lines.iter().enumerate() {
        if i == 0 {
            broken.push_str(line);
            broken.push('\n');
        } else if (mid..mid + 128).contains(&i) {
            broken.push_str("garbage,not-a-float,row\n");
        } else {
            broken.push_str(line);
            broken.push('\n');
        }
    }

    let mut source = SimSource::new(broken.as_bytes());
    let mut sink = VecSink::default();
    let summary = a::run_a(&mut source, "run1", &mut sink);
    assert!(summary.dirty_windows >= 1, "the corrupt stretch is dropped");
    assert!(
        summary.windows > 650,
        "the node keeps classifying after the corruption: {summary:?}"
    );
    assert!(!sink.0.is_empty(), "statuses keep flowing");
}

#[test]
fn node_q_verdicts_match_the_tap_truth() {
    let scenario = Scenario::parse(TAPS_TOML).expect("taps scenario");
    let (dataset, meta) = taps_csv(&scenario, 42);
    let mut source = TapSource::new(dataset.as_bytes(), meta.as_bytes());
    let mut sink = VecSink::default();
    let summary = q::run_q(&mut source, "run1", &mut sink);

    assert_eq!(summary.windows, summary.verdicts);
    assert_eq!(summary.dirty_windows, 0);
    assert!(!sink.0.is_empty());

    // Ground truth: the meta rows in order (t_ms, verdict).
    let truth: Vec<(u32, &str)> = meta
        .lines()
        .skip(1)
        .map(|l| {
            let mut fields = l.split(',');
            (
                fields.next().unwrap().parse().unwrap(),
                fields.next().unwrap(),
            )
        })
        .collect();
    assert_eq!(sink.0.len(), truth.len(), "one verdict per tap");
    let matches = sink
        .0
        .iter()
        .zip(&truth)
        .filter(|(row, (_, verdict))| row.state == *verdict)
        .count();
    let accuracy = matches as f32 / truth.len() as f32;
    assert!(
        accuracy >= 0.98,
        "verdict accuracy {accuracy:.3} on an unseen seed"
    );
}

#[test]
fn both_nodes_one_run_with_mqtt() {
    // The D5 coherent run: simulator streams -> both nodes -> loopback broker
    // + offline sinks, no crashes.
    let scenario = Scenario::parse(BASE_TOML).expect("base scenario");
    let csv = run_csv(&scenario, 42);
    let taps_scenario = Scenario::parse(TAPS_TOML).expect("taps scenario");
    let (dataset, meta) = taps_csv(&taps_scenario, 42);

    let broker = LoopbackBroker::spawn();
    let mut mqtt_a = MqttSink::new(&broker.addr, "node-a");
    let mut mqtt_q = MqttSink::new(&broker.addr, "node-q");

    let mut offline_a = VecSink::default();
    let mut offline_q = VecSink::default();

    let mut source_a = SimSource::new(csv.as_bytes());
    let summary_a = a::run_a(
        &mut source_a,
        "run1",
        &mut MultiSink(&mut offline_a, &mut mqtt_a),
    );
    let mut source_q = TapSource::new(dataset.as_bytes(), meta.as_bytes());
    let summary_q = q::run_q(
        &mut source_q,
        "run1",
        &mut MultiSink(&mut offline_q, &mut mqtt_q),
    );

    assert!(summary_a.windows > 0 && summary_q.windows > 0);
    assert!(!offline_a.0.is_empty() && !offline_q.0.is_empty());

    // Every offline row also reached the broker (statuses + verdicts).
    let expected = offline_a.0.len() + offline_q.0.len();
    let mut received = Vec::new();
    while received.len() < expected {
        match broker
            .publishes
            .recv_timeout(std::time::Duration::from_secs(5))
        {
            Ok(publish) => {
                assert!(publish.topic.starts_with("oee/line1/"));
                received.push(publish);
            }
            Err(_) => break,
        }
    }
    assert_eq!(
        received.len(),
        expected,
        "statuses must reach MQTT exactly once each"
    );
}

/// The belt-events CSV text for a scenario (the same rows the simulator
/// CLI writes via `--belt-events`).
fn belt_csv(scenario: &Scenario, seed: u64) -> (String, usize, usize) {
    let parts = belt::generate(scenario, seed);
    let doubles = parts.iter().filter(|part| part.pulses == 2).count();
    let mut events = Vec::new();
    belt::write_events_csv(&parts, &scenario.belt, &mut events).unwrap();
    (String::from_utf8(events).unwrap(), parts.len(), doubles)
}

#[test]
fn node_p_count_matches_the_belt_truth() {
    // The D1 gate: on a scenario with doubles and skips, count = truth.
    let scenario = Scenario::parse(BELT_TOML).expect("belt scenario");
    let (events, truth_parts, doubles) = belt_csv(&scenario, 42);
    assert!(doubles > 0, "the check must exercise doubles (seed 42)");
    assert!(
        truth_parts < (29_000 - 1_000) / 400,
        "the check must exercise skips ({} parts vs 70 slots)",
        truth_parts
    );

    let mut source = IrSource::new(events.as_bytes());
    let mut sink = VecSink::default();
    let summary = p::run_p(&mut source, "run1", &mut sink);

    assert_eq!(summary.parts as usize, truth_parts, "count = truth");
    assert_eq!(summary.merged, doubles, "one merge per double");
    assert_eq!(summary.bad_rows, 0);
    assert_eq!(sink.0.len(), truth_parts, "one status row per part");
    // The rows carry the cumulative count in `state`.
    assert_eq!(sink.0[0].state, "1");
    assert_eq!(sink.0.last().unwrap().state, truth_parts.to_string());
    // Determinism: a second pass over the same stream is identical.
    let mut source2 = IrSource::new(events.as_bytes());
    let mut sink2 = VecSink::default();
    let summary2 = p::run_p(&mut source2, "run1", &mut sink2);
    assert_eq!(summary, summary2);
    assert_eq!(sink.0, sink2.0);
}

#[test]
fn node_p_survives_corrupt_rows() {
    let scenario = Scenario::parse(BELT_TOML).expect("belt scenario");
    let (events, truth_parts, _) = belt_csv(&scenario, 42);
    // Corrupt every 10th event row: skipped, not fatal.
    let mut broken = String::new();
    for (i, line) in events.lines().enumerate() {
        if i % 10 == 3 {
            broken.push_str("garbage,row\n");
        } else {
            broken.push_str(line);
            broken.push('\n');
        }
    }
    let mut source = IrSource::new(broken.as_bytes());
    let mut sink = VecSink::default();
    let summary = p::run_p(&mut source, "run1", &mut sink);
    assert!(summary.bad_rows > 0, "the corrupt rows are counted");
    assert!(
        summary.parts < truth_parts as u32,
        "lost edges are lost parts (an honest gap, documented)"
    );
    assert!(
        !sink.0.is_empty(),
        "the node keeps counting after corruption"
    );
}

#[test]
fn node_p_publishes_counts_over_mqtt() {
    // The p/count topic shape + the end marker — the aggregator contract.
    let scenario = Scenario::parse(BELT_TOML).expect("belt scenario");
    let (events, _, _) = belt_csv(&scenario, 42);
    let broker = LoopbackBroker::spawn();
    let mut mqtt = MqttSink::new(&broker.addr, "node-p");
    mqtt.publish_p_meta();

    let mut source = IrSource::new(events.as_bytes());
    let mut offline = VecSink::default();
    let summary = p::run_p(
        &mut source,
        "belt42",
        &mut MultiSink(&mut offline, &mut mqtt),
    );
    let last_t = source.last_t_ms();
    mqtt.publish_end("p", last_t, "belt42");

    // Every offline row also reached the broker, plus meta + end.
    let expected = offline.0.len() + 2;
    let mut topics = Vec::new();
    while topics.len() < expected {
        match broker
            .publishes
            .recv_timeout(std::time::Duration::from_secs(5))
        {
            Ok(publish) => topics.push(publish),
            Err(_) => break,
        }
    }
    assert_eq!(topics.len(), expected);
    assert_eq!(topics[0].topic, "oee/line1/p/meta");
    let first_count = &topics[1];
    assert_eq!(first_count.topic, "oee/line1/p/count");
    assert_eq!(
        first_count.payload,
        format!(
            r#"{{"count":1,"t_ms":{},"run_id":"belt42"}}"#,
            offline.0[0].t_ms
        )
    );
    let end = topics.last().unwrap();
    assert_eq!(end.topic, "oee/line1/p/end");
    assert_eq!(
        end.payload,
        format!(r#"{{"t_ms":{last_t},"run_id":"belt42"}}"#)
    );
    assert_eq!(summary.parts as usize, offline.0.len());
}

#[test]
fn q_model_microflow_matches_the_interpreter_reference() {
    // Parity on the Q model: the export's naive float interpreter vs the
    // microflow `#[model]` inference must agree on the argmax over real tap
    // windows (the rust-ml track's parity discipline, applied to Q).
    let scenario = Scenario::parse(TAPS_TOML).expect("taps scenario");
    let events = taps::generate(&scenario, 99);
    let interp = exporter::interp::InterpModel::from_file("../ml/models/model_q.tflite".as_ref())
        .expect("loading model_q through the interp reference");

    let mut agreed = 0usize;
    let mut total = 0usize;
    for event in events.iter().take(24) {
        let probs = interp.run(&event.samples).expect("interp runs");
        let interp_verdict = probs
            .probabilities
            .iter()
            .enumerate()
            .max_by(|(_, x), (_, y)| x.partial_cmp(y).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let window: SMatrix<f32, { q::WINDOW }, 1> = SMatrix::from_fn(|t, _| event.samples[t]);
        let microflow_verdict = q::classify(&window);
        total += 1;
        agreed += (interp_verdict == microflow_verdict) as usize;
        // Both sides also agree with the ground truth on these clean taps.
        assert_eq!(
            microflow_verdict,
            event.verdict.class_index(),
            "a wrong verdict on a tap at t={}",
            event.t_ms
        );
    }
    assert_eq!(agreed, total, "interp and microflow argmax diverged");
}

#[test]
fn predict_latency_smoke() {
    // "Not a metric — not worse": 100 A-windows and 20 Q-windows classify
    // well under the line tempo (parts every 400 ms; A windows every 80 ms).
    let a_window = SMatrix::<f32, { a::WINDOW }, 1>::from_fn(|t, _| {
        let ts = t as f32 / 1600.0;
        (2.0f32 * core::f32::consts::PI * 50.0 * ts).sin() * 2.0
    });
    let q_window = SMatrix::<f32, { q::WINDOW }, 1>::from_fn(|t, _| {
        let ts = t as f32 / 16_000.0;
        0.8 * (-(ts * 1000.0) / 14.0).exp() * (2.0 * core::f32::consts::PI * 2400.0 * ts).sin()
    });
    let start = std::time::Instant::now();
    for _ in 0..100 {
        std::hint::black_box(a::classify(&a_window));
    }
    for _ in 0..20 {
        std::hint::black_box(q::classify(&q_window));
    }
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "smoke latency too high in debug builds: {elapsed:?}"
    );
}
