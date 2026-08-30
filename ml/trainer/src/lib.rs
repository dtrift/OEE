//! Rust-ML track trainer (see `tmp/docs/decompose/rust-ml.rus.md`): trains
//! model A with burn (NdArray + autodiff), exports float weights, then runs
//! the exporter's PTQ + writer — the whole pipeline in one command, zero
//! Python.

pub mod data;
pub mod model;
pub mod pipeline;
pub mod train;

pub const NUM_CLASSES: usize = 4;
pub const CLASS_NAMES: [&str; NUM_CLASSES] = ["idle", "run", "jam", "overload"];
pub const TIMESTEPS: usize = 128;
pub const CHANNELS: usize = 1;

/// The training seed — the same value the Python path uses
/// (`ml/scripts/train_model_a.py`), so both tracks are independently
/// deterministic with one shared knob.
pub const SEED: u64 = 2026;
