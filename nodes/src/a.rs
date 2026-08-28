//! Node A: machine current -> features -> 1D-CNN -> status (Availability).
//!
//! Week 3 (D6 bridge): inference runs through the local microflow fork via
//! the path dependency — the `#[model]` macro compiles inside the workspace.
//! Until the trained `model_a.tflite` lands (D5, needs the TF venv), the
//! spike model with the identical architecture serves as the stand-in; the
//! switch is a one-line path change pinned by the test below.

use microflow::model;
use nalgebra::SMatrix;

// The path is resolved at compile time relative to the workspace root
// (rustc's cwd for workspace builds) — see fork/NOTES.md, week 3.
#[model("ml/models/conv1d.tflite")]
struct CurrentModel;

/// Window length of the node A model (WindowSpec(A) = 128 @ 1.6 kHz,
/// `features_cli::window_spec`).
pub const WINDOW: usize = 128;

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
}
