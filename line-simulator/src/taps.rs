//! Tap-test channel (week 4, D3): every machined part gets a tap, and the
//! ring sound tells a good part from a cracked one — ground truth for node Q.
//!
//! Synthesis (the scenario's `[taps]` section): a damped sine at 16 kHz —
//! a good part rings higher and longer, a cracked one lower and faster, plus
//! noise; amplitude/frequency wander seeded per tap (the "distinguishable,
//! but not perfectly" rule, same as the current signal).
//!
//! Output contracts:
//! - the training windows CSV `label,state,x000..x1023` — the same family as
//!   the model A datasets (`dataset.rs`), consumed by the trainer's Q task;
//! - the meta CSV `t_ms,verdict` — the Q ground truth for run evaluation.
//!
//! Taps flow only while the machine is in `Run` (parts are produced then);
//! the state timeline comes from the same scenario events as the current
//! signal. Tap windows use their own clock (16 kHz), independent of the
//! 1.6 kHz current stream.

use std::io::Write;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use rand_distr::Normal;

use crate::fsm::MachineState;
use crate::scenario::{Scenario, Taps};

/// Tap-audio sample rate, Hz (the INMP441/I2S rate, `WindowSpec(Q)`).
pub const TAP_SAMPLE_RATE_HZ: u32 = 16_000;

/// Tap window in samples — 64 ms. Must match `features_cli::window_spec(Q)`
/// (the single source of truth); the simulator does not depend on
/// `features-cli`, mirroring the model A precedent in `main.rs`.
pub const TAP_WINDOW: usize = 1024;

/// Tap verdict (the Q ground truth).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// A good part: rings high, decays slowly.
    Good,
    /// A cracked part: rings lower, decays fast, noisier.
    Cracked,
}

impl Verdict {
    /// Stable string name for CSV/logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Cracked => "cracked",
        }
    }

    /// Training class index (the Q model contract: 0=good, 1=cracked).
    pub const fn class_index(self) -> usize {
        match self {
            Self::Good => 0,
            Self::Cracked => 1,
        }
    }
}

/// One tap event: the part's arrival time, the verdict, and the synthesized
/// audio window (`TAP_WINDOW` samples @ `TAP_SAMPLE_RATE_HZ`, linear floats —
/// the "WAV-like buffer" of the decomposition, in CSV form).
#[derive(Debug, Clone, PartialEq)]
pub struct TapEvent {
    /// Part arrival time, ms (machine timeline).
    pub t_ms: u32,
    /// Ground-truth verdict.
    pub verdict: Verdict,
    /// The audio window samples (relative units).
    pub samples: Vec<f32>,
}

/// Generates all tap events of the scenario (seeded, deterministic).
///
/// One RNG stream separate from the current signal: the same seed yields the
/// same taps regardless of whether the current CSV was also requested.
pub fn generate(scenario: &Scenario, seed: u64) -> Vec<TapEvent> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut events = Vec::new();
    let mut state = MachineState::Idle;
    let mut next_event = 0usize;
    let period = scenario.taps.period_ms.max(1);
    let mut t_ms = period;
    while t_ms < scenario.duration_ms {
        while next_event < scenario.events.len() && scenario.events[next_event].t_ms <= t_ms {
            state = scenario.events[next_event].state;
            next_event += 1;
        }
        if state == MachineState::Run {
            let verdict = if rng.random::<f32>() < scenario.taps.crack_probability {
                Verdict::Cracked
            } else {
                Verdict::Good
            };
            let samples = synth_window(&mut rng, verdict, &scenario.taps);
            events.push(TapEvent {
                t_ms,
                verdict,
                samples,
            });
        }
        t_ms += period;
    }
    events
}

/// Synthesizes one tap window: a damped sine with seeded parameter wander.
fn synth_window(rng: &mut StdRng, verdict: Verdict, taps: &Taps) -> Vec<f32> {
    let (base_freq, tau_ms) = match verdict {
        Verdict::Good => (taps.good_freq_hz, taps.good_tau_ms),
        Verdict::Cracked => (taps.cracked_freq_hz, taps.cracked_tau_ms),
    };
    // Inclusive ranges: a zero jitter yields a single-point range instead
    // of an empty (panicking) one.
    let freq = base_freq * (1.0 + rng.random_range(-taps.freq_jitter..=taps.freq_jitter));
    let amplitude = taps.amplitude * (1.0 + rng.random_range(-taps.amp_jitter..=taps.amp_jitter));
    // A crack adds broadband rattle on top of the ring.
    let noise_sigma = taps.noise_sigma
        * if verdict == Verdict::Cracked {
            taps.crack_noise_boost
        } else {
            1.0
        };
    let ring = Normal::new(0.0, noise_sigma).expect("sigma >= 0");
    (0..TAP_WINDOW)
        .map(|i| {
            let t_s = i as f32 / TAP_SAMPLE_RATE_HZ as f32;
            let envelope = (-(t_s * 1000.0) / tau_ms).exp();
            amplitude * envelope * (2.0 * core::f32::consts::PI * freq * t_s).sin()
                + rng.sample(ring)
        })
        .collect()
}

/// Writes the training-windows CSV; returns the number of data rows.
pub fn write_dataset_csv(events: &[TapEvent], mut writer: impl Write) -> std::io::Result<usize> {
    let mut header = vec!["label".to_string(), "state".to_string()];
    for i in 0..TAP_WINDOW {
        header.push(format!("x{i:03}"));
    }
    let mut text = String::new();
    text.push_str(&header.join(","));
    text.push('\n');
    for event in events {
        let mut row = vec![
            event.verdict.class_index().to_string(),
            event.verdict.as_str().to_string(),
        ];
        row.extend(event.samples.iter().map(|s| format!("{s:.6}")));
        text.push_str(&row.join(","));
        text.push('\n');
    }
    writer.write_all(text.as_bytes())?;
    Ok(events.len())
}

/// Writes the ground-truth meta CSV (`t_ms,verdict`); returns the row count.
pub fn write_meta_csv(events: &[TapEvent], mut writer: impl Write) -> std::io::Result<usize> {
    let mut text = String::from("t_ms,verdict\n");
    for event in events {
        text.push_str(&event.t_ms.to_string());
        text.push(',');
        text.push_str(event.verdict.as_str());
        text.push('\n');
    }
    writer.write_all(text.as_bytes())?;
    Ok(events.len())
}

/// Verdict histogram `[good, cracked]` — the dataset balance check.
pub fn verdict_histogram(events: &[TapEvent]) -> [usize; 2] {
    let mut histogram = [0usize; 2];
    for event in events {
        histogram[event.verdict.class_index()] += 1;
    }
    histogram
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario_text(body: &str) -> Scenario {
        Scenario::parse(&format!("duration_ms = 10000\n{body}")).expect("scenario")
    }

    fn run_scenario() -> Scenario {
        scenario_text(
            r#"
[[events]]
t_ms = 100
state = "Run"
[[events]]
t_ms = 6000
state = "Idle"
"#,
        )
    }

    #[test]
    fn same_seed_same_bits() {
        let scenario = run_scenario();
        let bits = |seed: u64| {
            generate(&scenario, seed)
                .iter()
                .map(|e| {
                    (
                        e.t_ms,
                        e.verdict,
                        e.samples.iter().map(|s| s.to_bits()).collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(bits(42), bits(42));
        assert_ne!(bits(42), bits(43));
    }

    #[test]
    fn taps_flow_only_in_run() {
        // Run spans [100, 6000) ms; the default period is 400 ms.
        let events = generate(&run_scenario(), 42);
        assert!(!events.is_empty(), "the run window must produce taps");
        assert!(events.iter().all(|e| (100..6000).contains(&e.t_ms)));
        // Tap slots outside Run are skipped entirely: 400..=5600 inside,
        // 6000+ outside — the count matches the in-run slots.
        assert_eq!(events.len(), (5600 - 400) / 400 + 1);
    }

    #[test]
    fn good_rings_longer_than_cracked() {
        // Fixed parameters (no wander, little noise): the good part's tail
        // must dominate the cracked one's — the physical separability the
        // Q model relies on.
        let scenario = scenario_text(&format!(
            r#"
[[events]]
t_ms = 1
state = "Run"
[taps]
freq_jitter = 0.0
amp_jitter = 0.0
noise_sigma = 0.001
good_tau_ms = {}
cracked_tau_ms = {}
"#,
            Taps::default().good_tau_ms,
            Taps::default().cracked_tau_ms,
        ));
        let events = generate(&scenario, 7);
        let tail_rms = |e: &TapEvent| {
            let tail = &e.samples[TAP_WINDOW / 2..];
            let sum = tail.iter().map(|s| s * s).sum::<f32>();
            (sum / tail.len() as f32).sqrt()
        };
        let good = events
            .iter()
            .find(|e| e.verdict == Verdict::Good)
            .expect("a good tap");
        let cracked = events.iter().find(|e| e.verdict == Verdict::Cracked);
        let Some(cracked) = cracked else {
            panic!(
                "seed 7 must yield both classes: {:?}",
                verdict_histogram(&events)
            );
        };
        assert!(
            tail_rms(good) > 4.0 * tail_rms(cracked),
            "good tail {} vs cracked tail {}",
            tail_rms(good),
            tail_rms(cracked)
        );
    }

    #[test]
    fn both_classes_present_at_balanced_probability() {
        let scenario = scenario_text(
            r#"
[[events]]
t_ms = 1
state = "Run"
[taps]
crack_probability = 0.5
"#,
        );
        // 10 s of run at the default 400 ms period gives ~24 taps — enough
        // for both classes at p=0.5 to appear (checked over several seeds).
        for seed in [2026, 7, 99] {
            let histogram = verdict_histogram(&generate(&scenario, seed));
            assert!(histogram[0] > 0, "seed {seed}: good missing");
            assert!(histogram[1] > 0, "seed {seed}: cracked missing");
        }
    }

    #[test]
    fn dataset_csv_header_is_pinned() {
        let events = generate(&run_scenario(), 42);
        let mut buffer = Vec::new();
        let rows = write_dataset_csv(&events, &mut buffer).unwrap();
        assert_eq!(rows, events.len());
        let text = String::from_utf8(buffer).unwrap();
        let mut lines = text.lines();
        let header = lines.next().unwrap();
        let columns = header.split(',').collect::<Vec<_>>();
        assert_eq!(columns.len(), 2 + TAP_WINDOW);
        assert_eq!(columns[0], "label");
        assert_eq!(columns[1], "state");
        assert_eq!(columns[2], "x000");
        assert_eq!(columns.last(), Some(&"x1023"));
        let first_row = lines.next().unwrap().split(',').collect::<Vec<_>>();
        assert_eq!(first_row.len(), 2 + TAP_WINDOW);
    }

    #[test]
    fn meta_csv_rows_match_events() {
        let events = generate(&run_scenario(), 42);
        let mut buffer = Vec::new();
        let rows = write_meta_csv(&events, &mut buffer).unwrap();
        assert_eq!(rows, events.len());
        let text = String::from_utf8(buffer).unwrap();
        assert_eq!(text.lines().next().unwrap(), "t_ms,verdict");
        let first = text.lines().nth(1).unwrap();
        assert_eq!(
            first,
            format!("{},{}", events[0].t_ms, events[0].verdict.as_str())
        );
    }
}
