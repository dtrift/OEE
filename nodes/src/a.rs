//! Node A: machine current -> features -> 1D-CNN -> status (Availability).
//!
//! Week 3 (D6 bridge): inference runs through the local microflow fork via
//! the path dependency — the `#[model]` macro compiles inside the workspace.
//! Rust-ML track (D5): the model is the rust-born `model_a.tflite` (burn
//! training → PTQ → flatbuffers writer, one command in `ml/`). The switch
//! from the TF-converted spike model is a one-line path change (the old
//! artifact stays as `conv1d.tflite`).
//!
//! Week 4 (D1/D2): the offline pipeline — a [`SimSource`](crate::sim_source)
//! stream cut into non-overlapping windows, classified, smoothed by the
//! anti-flap hysteresis, and emitted to a status sink (CSV today, MQTT on
//! top — see `mqtt_sink`).

use microflow::model;
use nalgebra::SMatrix;

use crate::sim_source::SimSource;
use crate::source::SensorSource;
use crate::status::{Hysteresis, StatusRow, StatusSink, WindowAccumulator, WindowOutcome};

// The path is resolved at compile time relative to the workspace root
// (rustc's cwd for workspace builds) — see fork/NOTES.md, week 3.
#[model("ml/models/model_a.tflite")]
struct CurrentModel;

/// Window length of the node A model (WindowSpec(A) = 128 @ 1.6 kHz,
/// `features_cli::window_spec`).
pub const WINDOW: usize = 128;

/// Status names by class index (`MachineState::class_index` order).
pub const STATE_NAMES: [&str; 4] = ["idle", "run", "jam", "overload"];

/// Consecutive agreeing windows before a status change is confirmed (D2,
/// anti-flap; 2 x 80 ms = 160 ms at the node A window rate).
pub const CONFIRM_AFTER: u32 = 2;

/// Node A purpose.
pub fn describe() -> &'static str {
    "node A: current -> features -> conv1d -> state"
}

/// Classifies a raw current window (amperes) into a machine-state index
/// (0=idle, 1=run, 2=jam, 3=overload — `MachineState::class_index`).
pub fn classify(window: &SMatrix<f32, WINDOW, 1>) -> usize {
    let output = CurrentModel::predict(*window);
    output
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(index, _)| index)
        .unwrap_or(usize::MAX)
}

/// The offline node A run summary (gate D1/D5 numbers).
#[derive(Debug, Default, PartialEq)]
pub struct RunSummary {
    /// Clean windows classified.
    pub windows: usize,
    /// Windows dropped because of bad samples (error isolation).
    pub dirty_windows: usize,
    /// Status rows emitted (changes + initial).
    pub statuses: usize,
}

/// Runs node A over a run-CSV source with the default anti-flap
/// hysteresis (see [`CONFIRM_AFTER`]).
pub fn run_a<R: std::io::Read>(
    source: &mut SimSource<R>,
    run_id: &str,
    sink: &mut dyn StatusSink,
) -> RunSummary {
    run_a_confirmed(source, run_id, sink, CONFIRM_AFTER)
}

/// [`run_a`] with an explicit hysteresis depth — the week-5 D5 sensitivity
/// sweep (1 window = fast but flappy, 2 = the line default, 3 = calm but
/// blind to short episodes).
pub fn run_a_confirmed<R: std::io::Read>(
    source: &mut SimSource<R>,
    run_id: &str,
    sink: &mut dyn StatusSink,
    confirm_after: u32,
) -> RunSummary {
    let mut accumulator = WindowAccumulator::new(WINDOW);
    let mut hysteresis = Hysteresis::new(confirm_after);
    let mut summary = RunSummary::default();
    // Exhaustion ends the run; a partial trailing window is dropped.
    while let Ok(sample) = source.next_sample() {
        // A dirty stretch (bad/NaN rows) poisons the window, not the run.
        let dirty = source.take_dirty();
        let outcome = accumulator.push(if dirty { None } else { Some(sample) }, WINDOW);
        match outcome {
            WindowOutcome::Complete(window) => {
                summary.windows += 1;
                let matrix: SMatrix<f32, WINDOW, 1> = SMatrix::from_fn(|t, _| window[t]);
                let prediction = classify(&matrix);
                let t_ms = source.last_t_ms();
                if let Some(status) = hysteresis.observe(prediction) {
                    sink.on_status(&StatusRow {
                        node: "a",
                        run_id: run_id.to_string(),
                        t_ms,
                        state: STATE_NAMES[status.min(STATE_NAMES.len() - 1)].to_string(),
                    });
                    summary.statuses += 1;
                }
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

    /// A synthetic current window at the given amplitude (the simulator's
    /// carrier shape: 50 Hz + harmonics).
    fn window(amplitude: f32) -> SMatrix<f32, WINDOW, 1> {
        SMatrix::from_fn(|t, _| {
            let ts = t as f32 / 1600.0;
            let mains = (2.0 * core::f32::consts::PI * 50.0 * ts).sin();
            let third = (2.0 * core::f32::consts::PI * 150.0 * ts).sin();
            let fifth = (2.0 * core::f32::consts::PI * 250.0 * ts).sin();
            amplitude * (mains + 0.15 * third + 0.07 * fifth)
        })
    }

    #[test]
    fn model_builds_through_the_bridge() {
        // The week-3 gate artifact: the fork macro expands inside the
        // workspace and predicts deterministically.
        let first = classify(&window(1.0));
        let second = classify(&window(1.0));
        assert_eq!(first, second);
        assert!(first < 4, "class index out of range: {first}");
    }

    #[test]
    fn model_separates_idle_from_run() {
        // The rust-born model must at least tell idle from run on clean
        // synthetic carriers (the D1 sanity behind the truth comparison).
        let idle = classify(&window(0.4));
        let run = classify(&window(2.0));
        assert_eq!(idle, 0, "idle window classified as {idle}");
        assert_eq!(run, 1, "run window classified as {run}");
    }
}
