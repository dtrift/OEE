//! Node Q: tap test -> audio -> 1D-CNN -> pass/fail (Quality).
//!
//! Week 4 (D4): the model is the rust-born `model_q.tflite` (the rust-ml
//! pipeline, `--task q`; burn training on the tap datasets of
//! `line-simulator --taps-dataset`). One tap = exactly one window of
//! `WindowSpec(Q)` (1024 @ 16 kHz) — no window sliding, no hysteresis: parts
//! are independent events.
//!
//! Verdict mapping (pinned by `line_simulator::taps::Verdict`):
//! 0 = good, 1 = cracked.

use microflow::model;
use nalgebra::SMatrix;

use crate::sim_source::TapSource;
use crate::source::SensorSource;
use crate::status::{StatusRow, StatusSink, WindowAccumulator, WindowOutcome};

// The path is resolved at compile time relative to the workspace root
// (rustc's cwd for workspace builds) — see fork/NOTES.md, week 3.
#[model("ml/models/model_q.tflite")]
struct TapModel;

/// Window length of the node Q model (WindowSpec(Q) = 1024 @ 16 kHz,
/// `features_cli::window_spec`).
pub const WINDOW: usize = 1024;

/// Verdict names by class index (`taps::Verdict::as_str` order).
pub const VERDICT_NAMES: [&str; 2] = ["good", "cracked"];

/// Node Q purpose.
pub fn describe() -> &'static str {
    "node Q: tap test -> audio -> conv1d -> verdict"
}

/// Classifies a tap window (relative units) into a verdict index
/// (0=good, 1=cracked).
pub fn classify(window: &SMatrix<f32, WINDOW, 1>) -> usize {
    let output = TapModel::predict(*window);
    output
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(index, _)| index)
        .unwrap_or(usize::MAX)
}

/// The offline node Q run summary (gate D4/D5 numbers).
#[derive(Debug, Default, PartialEq)]
pub struct RunSummary {
    /// Tap windows classified.
    pub windows: usize,
    /// Windows dropped because of bad rows (error isolation).
    pub dirty_windows: usize,
    /// Verdict rows emitted.
    pub verdicts: usize,
}

/// Runs node Q over a tap-dataset source: one window per tap -> classify ->
/// the sink (no hysteresis: parts are independent events).
pub fn run_q<R: std::io::Read, M: std::io::Read>(
    source: &mut TapSource<R, M>,
    run_id: &str,
    sink: &mut dyn StatusSink,
) -> RunSummary {
    let mut accumulator = WindowAccumulator::new(WINDOW);
    let mut summary = RunSummary::default();
    // Exhaustion ends the run; a partial trailing window is dropped.
    while let Ok(sample) = source.next_sample() {
        let dirty = source.take_dirty();
        let outcome = accumulator.push(if dirty { None } else { Some(sample) }, WINDOW);
        match outcome {
            WindowOutcome::Complete(window) => {
                summary.windows += 1;
                let matrix: SMatrix<f32, WINDOW, 1> = SMatrix::from_fn(|t, _| window[t]);
                let verdict = classify(&matrix);
                let t_ms = source.last_t_ms();
                sink.on_status(&StatusRow {
                    node: "q",
                    run_id: run_id.to_string(),
                    t_ms,
                    state: VERDICT_NAMES[verdict.min(VERDICT_NAMES.len() - 1)].to_string(),
                });
                summary.verdicts += 1;
            }
            WindowOutcome::Dirty => {
                summary.dirty_windows += 1;
            }
            WindowOutcome::Filling => {}
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_builds_and_is_deterministic() {
        // A flat zero window still yields a stable verdict.
        let window = SMatrix::<f32, WINDOW, 1>::zeros();
        let first = classify(&window);
        let second = classify(&window);
        assert_eq!(first, second);
        assert!(first < 2, "verdict index out of range: {first}");
    }
}
