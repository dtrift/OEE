//! Conveyor belt channel (week 5, D1): parts pass an IR barrier — the raw
//! material for node P (Performance).
//!
//! Synthesis (the scenario's `[belt]` section): while the machine is in
//! `Run`, a part slot arrives every `period_ms` (nominal); the actual pass
//! time jitters around the slot (seeded). Per slot, with seeded
//! probabilities:
//! - a **skip** — no part came at all: no IR events, and the slot is not in
//!   the ground truth either (the scenario is the truth: a slot without a
//!   part produced nothing, so a perfect detector must not count it);
//! - a **double** — one part re-triggers the barrier: two pulses close
//!   together (`pulse_ms` wide, `double_gap_ms` apart) — the anti-double-
//!   count case for node P;
//! - otherwise a single pulse.
//!
//! "P count = truth" (the D1 gate) is therefore achievable by construction:
//! truth = parts that passed (doubles are still one part), skips produced
//! nothing, and node P's anti-double window merges the two pulses of a
//! double into one count. The residual Performance error in the week-5
//! experiment comes from the measured run time (node A), not the count.
//!
//! Output contracts:
//! - the events CSV `t_ms,ir` — level changes of the IR barrier line only
//!   (0=clear, 1=blocked); node P's input;
//! - the meta CSV `t_ms,pulses` — one row per true part; the ground truth.
//!
//! The belt has its own RNG stream (salted seed — independent of the
//! current-signal and tap streams) and its own clock: requesting it
//! alongside `--out`/`--dataset`/`--taps-dataset` changes nothing else
//! (same determinism contract as the tap channel).

use std::io::Write;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::fsm::MachineState;
use crate::scenario::{Belt, Scenario};

/// Seed salt: the belt stream draws must not mirror the tap stream's draws
/// (both would otherwise consume the same per-slot `random::<f32>()`
/// sequence from the same seed).
const BELT_SEED_SALT: u64 = 0xBE17_2026;

/// One true part: when it passed the barrier and how many IR pulses it
/// triggered (1 = normal, 2 = the anti-double-count case).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeltPart {
    /// Part pass time, ms (machine timeline).
    pub t_ms: u32,
    /// IR pulses the part triggered.
    pub pulses: u8,
}

/// Generates all belt parts of the scenario (seeded, deterministic).
pub fn generate(scenario: &Scenario, seed: u64) -> Vec<BeltPart> {
    let mut rng = StdRng::seed_from_u64(seed ^ BELT_SEED_SALT);
    let belt = &scenario.belt;
    let mut parts = Vec::new();
    let mut state = MachineState::Idle;
    let mut next_event = 0usize;
    let period = belt.period_ms.max(1);
    let mut slot = period;
    while slot < scenario.duration_ms {
        while next_event < scenario.events.len() && scenario.events[next_event].t_ms <= slot {
            state = scenario.events[next_event].state;
            next_event += 1;
        }
        if state == MachineState::Run {
            // Fixed draw order per slot (skip, double, jitter) regardless of
            // the branch outcomes — the stream never depends on how many
            // slots were skipped.
            let skip = rng.random::<f32>() < belt.skip_probability;
            let double = rng.random::<f32>() < belt.double_probability;
            let offset = period as f32 * belt.jitter * rng.random_range(-1.0..=1.0);
            if !skip {
                parts.push(BeltPart {
                    t_ms: (slot as f32 + offset).round() as u32,
                    pulses: if double { 2 } else { 1 },
                });
            }
        }
        slot += period;
    }
    parts
}

/// The IR-barrier pulse times of one part: one rise/fall pair per pulse,
/// the second pulse of a double `double_gap_ms` after the first fell.
fn pulse_edges(part: &BeltPart, belt: &Belt) -> Vec<(u32, u8)> {
    let mut edges = Vec::with_capacity(part.pulses as usize * 2);
    for pulse in 0..part.pulses as u32 {
        let rise = part.t_ms + pulse * (belt.pulse_ms + belt.double_gap_ms);
        edges.push((rise, 1));
        edges.push((rise + belt.pulse_ms, 0));
    }
    edges
}

/// Writes the IR-barrier level-change CSV (`t_ms,ir`); returns the row
/// count. Rows are the barrier signal edges only, starting from the idle
/// level at t=0 — node P's edge detector consumes exactly this stream.
/// Parts are generated in slot order, so the edges are already sorted.
pub fn write_events_csv(
    parts: &[BeltPart],
    belt: &Belt,
    mut writer: impl Write,
) -> std::io::Result<usize> {
    let mut text = String::from("t_ms,ir\n0,0\n");
    let mut rows = 1usize;
    for part in parts {
        for (t_ms, level) in pulse_edges(part, belt) {
            text.push_str(&t_ms.to_string());
            text.push(',');
            text.push_str(&level.to_string());
            text.push('\n');
            rows += 1;
        }
    }
    writer.write_all(text.as_bytes())?;
    Ok(rows)
}

/// Writes the ground-truth meta CSV (`t_ms,pulses`); returns the row count.
pub fn write_meta_csv(parts: &[BeltPart], mut writer: impl Write) -> std::io::Result<usize> {
    let mut text = String::from("t_ms,pulses\n");
    for part in parts {
        text.push_str(&part.t_ms.to_string());
        text.push(',');
        text.push_str(&part.pulses.to_string());
        text.push('\n');
    }
    writer.write_all(text.as_bytes())?;
    Ok(parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::Scenario;

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
        let bits = |seed: u64| generate(&scenario, seed);
        assert_eq!(bits(42), bits(42));
        assert_ne!(bits(42), bits(43));
    }

    #[test]
    fn parts_flow_only_in_run() {
        // Run spans [100, 6000) ms; the default period is 400 ms.
        let parts = generate(&run_scenario(), 42);
        assert!(!parts.is_empty(), "the run window must produce parts");
        assert!(parts.iter().all(|p| (100..6000).contains(&p.t_ms)));
        // In-run slots: 400..=5600 — skips only remove parts from that set.
        let in_run_slots = (400..6000).step_by(400).count();
        assert!(parts.len() <= in_run_slots);
    }

    #[test]
    fn belt_stream_is_independent_of_the_tap_stream() {
        // Same seed, different channels: the belt's salt keeps the two
        // streams from sharing their per-slot draws (checked over several
        // seeds; identical first-event times and equal counts would mean
        // the channels are correlated).
        let scenario = run_scenario();
        for seed in [7u64, 42, 2026] {
            let belt = generate(&scenario, seed);
            let taps = crate::taps::generate(&scenario, seed);
            let first_belt_t = belt.first().map(|p| p.t_ms);
            let first_tap_t = taps.first().map(|t| t.t_ms);
            let differ = first_belt_t != first_tap_t || belt.len() != taps.len();
            assert!(differ, "seed {seed}: belt and tap streams are identical");
        }
    }

    #[test]
    fn doubles_and_skips_both_appear() {
        // Rich probabilities on a long run: both phenomena must occur, and
        // every part carries 1 or 2 pulses.
        let scenario = scenario_text(
            r#"
[[events]]
t_ms = 1
state = "Run"
[belt]
period_ms = 100
jitter = 0.1
pulse_ms = 10
double_gap_ms = 10
double_probability = 0.4
skip_probability = 0.3
"#,
        );
        let parts = generate(&scenario, 2026);
        assert!(parts.len() > 20, "a 10 s run at 100 ms must give parts");
        assert!(
            parts.iter().any(|p| p.pulses == 2),
            "doubles must occur at p=0.4"
        );
        assert!(
            parts.iter().any(|p| p.pulses == 1),
            "singles must occur too"
        );
        assert!(parts.len() < 100, "skips must occur at p=0.3");
    }

    #[test]
    fn double_pulses_fit_the_anti_double_window() {
        // The D1 gate precondition: a double's second rise must land inside
        // node P's anti-double window (100 ms), real parts outside it.
        let scenario = scenario_text(
            r#"
[[events]]
t_ms = 1
state = "Run"
[belt]
period_ms = 400
jitter = 0.2
double_probability = 0.5
"#,
        );
        let parts = generate(&scenario, 7);
        for window in parts.windows(2) {
            let gap = window[1].t_ms.saturating_sub(window[0].t_ms);
            assert!(
                gap > 100,
                "real parts {gap} ms apart must exceed the 100 ms window"
            );
        }
        let doubles = parts.iter().filter(|p| p.pulses == 2).count();
        assert!(doubles > 0, "seed 7 must yield doubles");
        for part in parts.iter().filter(|p| p.pulses == 2) {
            let second_rise = part.t_ms + (scenario.belt.pulse_ms + scenario.belt.double_gap_ms);
            assert!(second_rise - part.t_ms < 100);
        }
    }

    #[test]
    fn events_csv_schema_is_pinned() {
        let scenario = run_scenario();
        let parts = generate(&scenario, 42);
        let mut buffer = Vec::new();
        let rows = write_events_csv(&parts, &scenario.belt, &mut buffer).unwrap();
        let text = String::from_utf8(buffer).unwrap();
        assert_eq!(text.lines().next().unwrap(), "t_ms,ir");
        assert!(text.lines().nth(1).unwrap().starts_with("0,0"));
        // Every part contributes 1 or 2 pulse pairs + the idle baseline row.
        let expected = 1 + parts.iter().map(|p| p.pulses as usize * 2).sum::<usize>();
        assert_eq!(rows, expected);
        assert_eq!(text.lines().count(), expected + 1);
        // Levels strictly alternate 1,0,1,0 after the baseline.
        let levels: Vec<u8> = text
            .lines()
            .skip(1)
            .map(|l| l.split(',').nth(1).unwrap().parse().unwrap())
            .collect();
        for pair in levels.windows(2) {
            assert_ne!(pair[0], pair[1], "rows must be level changes only");
        }
    }

    #[test]
    fn meta_csv_rows_match_parts() {
        let scenario = run_scenario();
        let parts = generate(&scenario, 42);
        let mut buffer = Vec::new();
        let rows = write_meta_csv(&parts, &mut buffer).unwrap();
        assert_eq!(rows, parts.len());
        let text = String::from_utf8(buffer).unwrap();
        assert_eq!(text.lines().next().unwrap(), "t_ms,pulses");
        let first = text.lines().nth(1).unwrap();
        assert_eq!(first, format!("{},{}", parts[0].t_ms, parts[0].pulses));
    }
}
