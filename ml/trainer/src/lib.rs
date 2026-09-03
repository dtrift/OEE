//! Rust-ML track trainer (see `tmp/docs/decompose/rust-ml.rus.md`): trains
//! model A with burn (NdArray + autodiff), exports float weights, then runs
//! the exporter's PTQ + writer — the whole pipeline in one command, zero
//! Python.
//!
//! Week 4 (D4): the same pipeline serves node Q — the architecture is the
//! Conv1D family with a different input window and head, expressed as a
//! [`TaskSpec`] threaded through data/training/export.

pub mod data;
pub mod model;
pub mod pipeline;
pub mod train;

/// The model A class set (the `MachineState::class_index` order).
pub const NUM_CLASSES: usize = 4;
pub const CLASS_NAMES: [&str; NUM_CLASSES] = ["idle", "run", "jam", "overload"];
pub const TIMESTEPS: usize = 128;
pub const CHANNELS: usize = 1;

/// The model Q class set (the `taps::Verdict::class_index` order).
pub const Q_TIMESTEPS: usize = 1024;
pub const Q_NUM_CLASSES: usize = 2;
pub const Q_CLASS_NAMES: [&str; Q_NUM_CLASSES] = ["good", "cracked"];

/// The training seed — the same value the Python path uses
/// (`ml/scripts/train_model_a.py`), so both tracks are independently
/// deterministic with one shared knob.
pub const SEED: u64 = 2026;

/// Per-task model contract: window length, head size, class names.
///
/// Single source of truth for the dataset column count, the tensor shape,
/// the confusion matrix, and the metrics header — one struct instead of
/// scattered constants.
#[derive(Clone, Copy, Debug)]
pub struct TaskSpec {
    /// Input window in samples (`WindowSpec(kind).samples`).
    pub timesteps: usize,
    /// Head size.
    pub num_classes: usize,
    /// Class names, index order pinned by the ground-truth enums.
    pub class_names: &'static [&'static str],
}

impl TaskSpec {
    /// Node A: machine current, 128 @ 1.6 kHz -> idle/run/jam/overload.
    pub const fn a() -> Self {
        Self {
            timesteps: TIMESTEPS,
            num_classes: NUM_CLASSES,
            class_names: &CLASS_NAMES,
        }
    }

    /// Node Q: tap audio, 1024 @ 16 kHz -> good/cracked.
    pub const fn q() -> Self {
        Self {
            timesteps: Q_TIMESTEPS,
            num_classes: Q_NUM_CLASSES,
            class_names: &Q_CLASS_NAMES,
        }
    }

    /// Short task name (metrics files, logs).
    pub const fn name(&self) -> &'static str {
        match self.num_classes {
            NUM_CLASSES => "a",
            _ => "q",
        }
    }
}
