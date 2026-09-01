//! Rust-ML track exporter (see `tmp/docs/decompose/rust-ml.rus.md`): everything
//! between "float weights" and "int8 `.tflite`" that the week-3 pipeline used
//! to get from TensorFlow's converter.
//!
//! Layout (all burn-free, offline-buildable):
//! - [`graph`] — a typed, validating builder of the minimal 6-operator graph
//!   (no EXPAND_DIMS/RESHAPE/Flatten wrappers: the microflow parser folds
//!   those itself, `fork/docs/conv1d-spec.md` §2.1);
//! - [`writer`] — flatbuffers assembly through the fork's builder API
//!   (the generated bindings are included from the fork, see `vendor` below);
//! - [`dumper`] — a human-readable structure dump (our `conv1d_ops.txt`);
//! - [`quant`] — post-training quantization (PTQ) with the TF conventions
//!   from the week-1 serialization facts (F5–F7);
//! - [`interp`] — a naive float evaluator over a `.tflite` (the parity
//!   reference that replaces the TF interpreter);
//! - [`weights`] — the minimal float-weights format `model_a.float`
//!   (JSON header + f32 LE, no serde).

pub mod dumper;
pub mod graph;
pub mod interp;
pub mod quant;
// The flatbuffers bindings for the TFLite schema (reader + builder) are NOT
// copied: this is the very file the fork's `#[model]` macro compiles against
// (microflow-macros includes it the same way), included by path — one source
// of truth, zero duplication. Regeneration (if the fork ever updates it):
// `flatc --rust schema.fbs` in the fork; do not edit by hand.
#[path = "../../../fork/microflow/microflow-macros/flatbuffers/tflite_generated.rs"]
#[allow(unused_imports)]
#[allow(clippy::all)]
#[allow(mismatched_lifetime_syntaxes)]
mod vendor;
pub mod weights;
pub mod writer;
pub use vendor::tflite;
