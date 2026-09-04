//! The dense engine floor (week 6, D3).
//!
//! The week-1 serialization spike model (`ml/models/dense.tflite`,
//! Dense(16,relu)->Dense(4)->Softmax over an 8-value input, random
//! weights — see `spike/conv1d-serialization.md`). It was never trained on
//! node A data: this binary is a *size probe* — what the fork costs with
//! only the FC+softmax path, no convolution kernel at all — not a task
//! baseline (the report says so explicitly).

#![no_std]
#![no_main]

use core::fmt::Write as _;

use cortex_m::asm::nop;
use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{exit, EXIT_SUCCESS};
use microflow::model;
use nalgebra::matrix;
use panic_halt as _;

#[path = "../windows.rs"]
mod windows;

#[model("../ml/models/dense.tflite")]
struct DenseSpike;

#[entry]
fn main() -> ! {
    let mut uart = oee_qemu::uart::Uart0;
    uart.write_line("dense engine floor: dense.tflite (week-1 serialization toy, random weights)");
    // Input = the first 8 samples of the first fixed window. Arbitrary but
    // deterministic; the output values are meaningless by design.
    let head: [f32; 8] = core::array::from_fn(|i| windows::WINDOWS[0].samples[i]);
    let output = DenseSpike::predict(matrix![
        head[0], head[1], head[2], head[3], head[4], head[5], head[6], head[7]
    ]);
    let probs: [f32; 4] = core::array::from_fn(|i| output[i]);
    let argmax = probs
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(index, _)| index)
        .unwrap_or(usize::MAX);
    let _ = writeln!(
        uart,
        "dense: argmax={} probs=[{:.3} {:.3} {:.3} {:.3}]",
        argmax, probs[0], probs[1], probs[2], probs[3],
    );
    exit(EXIT_SUCCESS);
    loop {
        nop()
    }
}
