//! The host side of the QEMU parity check (week 6, D2).
//!
//! Prints the model A predictions for the same fixed windows as the
//! LM3S6965 firmware, in the same byte-stable format — `scripts/qemu-parity.sh`
//! diffs the two logs line by line. Also doubles as the demo's "host
//! reference" number: the bench's node A and this binary run the same
//! `#[model("ml/models/model_a.tflite")]` code.

// The firmware's generated windows, verbatim — one source of bits for both
// sides of the parity check (see scripts/gen-qemu-windows.py).
#[path = "../../qemu/src/windows.rs"]
mod windows;

use nalgebra::SMatrix;

use nodes::a::{classify_with_probs, STATE_NAMES, WINDOW};

fn main() {
    println!("OEE node A @ host: model_a.tflite, 4 fixed windows");
    for (index, window) in windows::WINDOWS.iter().enumerate() {
        let matrix = SMatrix::<f32, WINDOW, 1>::from_column_slice(&window.samples);
        let (probs, argmax) = classify_with_probs(&matrix);
        let bits: [u32; 4] = core::array::from_fn(|i| probs[i].to_bits());
        println!(
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
    println!("done");
}
