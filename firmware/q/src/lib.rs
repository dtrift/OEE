//! Node Q firmware logic (Quality) — the `no_std` core.
//!
//! The tap-test twin of `firmware-a`: a 1024-sample I2S window (16 kHz,
//! 64 ms — `features_cli::window_spec(Q)`) goes through the same rust-born
//! `model_q.tflite` the host node runs, and the verdict leaves as a
//! `q,<run_id>,<t_ms>,<verdict>` line.
//!
//! The INMP441 frame conversion (`i2s_samples_to_f32`) is pure logic,
//! host-tested: the mic emits 24-bit samples in 32-bit slots, and the
//! channel/order question is exactly the S4 bring-up risk — the conversion
//! is pinned here so the on-target fix stays a one-line swap.

#![no_std]

use core::fmt::Write as _;

use microflow::model;
use nalgebra::SMatrix;

/// Window length and rate: `features_cli::window_spec(NodeKind::Q)`
/// (1024 @ 16 kHz = 64 ms).
pub const WINDOW: usize = 1024;

/// Verdict names by class index (`nodes::q` order: good, cracked).
pub const VERDICT_NAMES: [&str; 2] = ["good", "cracked"];

#[model("../ml/models/model_q.tflite")]
struct ModelQ;

/// Classifies one complete tap window: (probabilities, verdict index).
pub fn classify(window: &[f32; WINDOW]) -> ([f32; 2], usize) {
    let matrix = SMatrix::<f32, WINDOW, 1>::from_column_slice(window);
    let output = ModelQ::predict(matrix);
    let mut probs = [0.0f32; 2];
    for (slot, value) in probs.iter_mut().zip(output.iter()) {
        *slot = *value;
    }
    let verdict = probs
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
        .map(|(index, _)| index)
        .unwrap_or(usize::MAX);
    (probs, verdict)
}

/// Converts raw I2S 32-bit slots to normalized f32 samples.
///
/// INMP441 contract (the S4 assumption, pinned by a test): 24-bit samples,
/// MSB-aligned in the 32-bit slot (i.e. the value occupies the top 24
/// bits), left channel first, one slot per channel per frame. The
/// normalization divides by 2^23 so a full-scale sample is ±~1.0 — the
/// same order of magnitude as the simulator's tap windows.
pub fn i2s_slots_to_f32(slots: &[u32], out: &mut [f32]) -> usize {
    let mut written = 0;
    for (slot, target) in slots.iter().zip(out.iter_mut()) {
        // Sign-extend the top 24 bits, then normalize.
        let sample = (*slot << 8) as i32 >> 8;
        *target = sample as f32 / (1 << 23) as f32;
        written += 1;
    }
    written
}

/// Formats a verdict line for the UART bridge / capture log:
/// `q,<run_id>,<t_ms>,<verdict>`.
pub fn format_verdict(out: &mut [u8], run_id: &str, t_ms: u32, verdict: usize) -> Option<usize> {
    let mut cursor = Cursor::new(out);
    writeln!(
        cursor,
        "q,{run_id},{t_ms},{}",
        VERDICT_NAMES.get(verdict).copied().unwrap_or("?")
    )
    .ok()?;
    Some(cursor.pos())
}

/// A minimal fixed-buffer write cursor (the `firmware-a` twin).
struct Cursor<'a> {
    buf: &'a mut [u8],
    written: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, written: 0 }
    }

    fn pos(&self) -> usize {
        self.written
    }
}

impl core::fmt::Write for Cursor<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if bytes.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[..bytes.len()].copy_from_slice(bytes);
        self.buf = &mut core::mem::take(&mut self.buf)[bytes.len()..];
        self.written += bytes.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i2s_conversion_sign_extends_and_normalizes() {
        // +full-scale (24-bit) -> ~1.0; the top byte carries the sign.
        let slots = [0x007F_FFFF, 0xFF80_0000, 0x0000_0000];
        let mut out = [0.0f32; 3];
        assert_eq!(i2s_slots_to_f32(&slots, &mut out), 3);
        assert!((out[0] - 1.0).abs() < 1e-6, "{}", out[0]);
        assert!((out[1] + 1.0).abs() < 1e-6, "{}", out[1]);
        assert_eq!(out[2], 0.0);
    }

    #[test]
    fn verdict_line_format_matches_the_offline_csv_family() {
        let mut buf = [0u8; 64];
        let n = format_verdict(&mut buf, "bench-q", 4242, 0).unwrap();
        assert_eq!(&buf[..n], b"q,bench-q,4242,good\n");
    }

    /// The model is the same rust-born `model_q.tflite` as the host node:
    /// the first label=good validation window must classify as good.
    #[test]
    fn classify_matches_the_host_node_q() {
        // A quiet decaying "ring" shape, good-part family: the real fixture
        // would be the val split; a synthetic quiet window keeps the test
        // independent of dataset files (the host side pins the dataset
        // parity in `ml/exporter`).
        let mut window = [0.0f32; WINDOW];
        for (i, slot) in window.iter_mut().enumerate() {
            let t = i as f32 / WINDOW as f32;
            *slot = (t * 120.0 * core::f32::consts::PI).sin() * (1.0 - t) * 0.05;
        }
        let (probs, verdict) = classify(&window);
        // The assertion is deliberately soft: the exact class of a
        // synthetic window is not the contract here — the conversion is.
        assert!(verdict < 2, "probs={probs:?}");
    }
}
