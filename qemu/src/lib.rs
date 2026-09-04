//! OEE node A on LM3S6965 (QEMU) — the week-6 portability demo.
//!
//! The same int8 model A that the host nodes run (`ml/models/model_a.tflite`)
//! is compiled into a `no_std` Cortex-M3 firmware: fixed windows in, class
//! probabilities + argmax out, over UART0. The output format is byte-stable
//! and mirrored by the host reference (`nodes/examples/qemu_host_ref.rs`) —
//! `scripts/qemu-parity.sh` diffs the two.
//!
//! Layout: `uart` (board output), `windows` (the generated fixed windows),
//! `run` (the shared demo body). The `conv2d` bin reuses `run` — it is the
//! same model, same windows, built with the fork's reshape-trick code path
//! (`MICROFLOW_CONV2D_ONLY=1`, scripts/footprint.sh).

#![no_std]

use core::fmt::Write as _;

use microflow::model;
use nalgebra::SMatrix;

pub mod uart;
pub mod windows;

/// Node A window length (`features_cli::window_spec`, 128 @ 1.6 kHz).
pub const WINDOW: usize = 128;

/// Class names in model A output order (`nodes::a::STATE_NAMES`).
pub const STATE_NAMES: [&str; 4] = ["idle", "run", "jam", "overload"];

// rustc's cwd for this standalone package is the crate root (see
// fork/NOTES.md, week 3, on path conventions).
#[model("../ml/models/model_a.tflite")]
struct ModelA;

/// Classifies one fixed window: (probabilities, argmax).
///
/// Mirrors `nodes::a::classify` — same `predict()` call on the same bits.
pub fn classify(window: &SMatrix<f32, WINDOW, 1>) -> ([f32; 4], usize) {
    let output = ModelA::predict(*window);
    let mut probs = [0.0f32; 4];
    for (slot, value) in probs.iter_mut().zip(output.iter()) {
        *slot = *value;
    }
    let argmax = probs
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(index, _)| index)
        .unwrap_or(usize::MAX);
    (probs, argmax)
}

/// Prints the model A row for one window (the byte-stable demo format).
fn print_window(uart: &mut uart::Uart0, index: usize, window: &windows::FixedWindow) {
    let matrix = SMatrix::<f32, WINDOW, 1>::from_column_slice(&window.samples);
    let (probs, argmax) = classify(&matrix);
    let bits: [u32; 4] = core::array::from_fn(|i| probs[i].to_bits());
    let _ = writeln!(
        uart,
        "win {index}: label={} argmax={} {} probs=[{:.3} {:.3} {:.3} {:.3}] bits=[{:#010x} {:#010x} {:#010x} {:#010x}]",
        window.label,
        argmax,
        STATE_NAMES[argmax],
        probs[0],
        probs[1],
        probs[2],
        probs[3],
        bits[0],
        bits[1],
        bits[2],
        bits[3],
    );
}

/// The demo body shared by the `oee-qemu` and `conv2d` binaries.
pub fn run() {
    let mut uart = uart::Uart0;
    uart.write_line("OEE node A @ LM3S6965 (QEMU): model_a.tflite, 4 fixed windows");
    for (index, window) in windows::WINDOWS.iter().enumerate() {
        print_window(&mut uart, index, window);
    }
    uart.write_line("done");
}
