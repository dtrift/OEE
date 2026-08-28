//! The node feature set (week 3, D4): RMS, peak, zero-crossings, spectrum.
//!
//! Role in the architecture (plan section 6): model A consumes raw windows
//! `(128, 1)` — these features are NOT a model input. They are a
//! dataset-analysis tool (separability checks, confusion diagnostics) and a
//! parity safety net: the same definitions are mirrored in
//! `ml/scripts/golden_features.py` (numpy) and locked by a golden test
//! (integer features bit-for-bit, float ±1e-6).
//!
//! `no_std` + fixed bins keep this compilable into node firmware.

use libm::{cosf, sqrtf};

/// Fundamental whose harmonics the spectrum bins track (the current signal
/// is a 50 Hz carrier, `line-simulator/src/signal.rs`).
pub const MAINS_HZ: f32 = 50.0;

/// Spectrum bins: mains harmonics 1, 3, 5 (amplitudes `third`/`fifth` are
/// simulator parameters). Fixed by design — no allocation, no FFT.
pub const SPECTRUM_HARMONICS: [u32; 3] = [1, 3, 5];

/// The extracted feature vector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Features {
    /// Root mean square of the window, A.
    pub rms: f32,
    /// Peak absolute value, A.
    pub peak: f32,
    /// Strict sign changes (`x[i-1] * x[i] < 0`).
    pub zero_crossings: u32,
    /// Goertzel magnitudes at [`SPECTRUM_HARMONICS`], A (normalized by N).
    pub spectrum: [f32; SPECTRUM_HARMONICS.len()],
}

/// Extracts the feature vector from a sample window.
///
/// `sample_rate_hz` selects the spectrum bins (e.g. 1600 for node A,
/// [`crate::window_spec`]); the window length is taken from the slice.
pub fn extract(window: &[f32], sample_rate_hz: u32) -> Features {
    let n = window.len();
    debug_assert!(n > 1, "a feature window needs at least 2 samples");

    let mut sum_squares = 0.0f32;
    let mut peak = 0.0f32;
    let mut zero_crossings = 0u32;
    for (i, sample) in window.iter().enumerate() {
        sum_squares += sample * sample;
        peak = peak.max(sample.abs());
        if i > 0 && window[i - 1] * sample < 0.0 {
            zero_crossings += 1;
        }
    }
    let spectrum = spectrum(window, sample_rate_hz);
    Features {
        rms: sqrtf(sum_squares / n as f32),
        peak,
        zero_crossings,
        spectrum,
    }
}

/// Goertzel magnitudes at the fixed harmonic bins.
///
/// Pure multiply-add recurrence per bin (no FFT, no allocation): the
/// coefficient is `2*cos(2*pi*f*n/fs)`, the magnitude is normalized by the
/// window length so it reads as an amplitude in A.
fn spectrum(window: &[f32], sample_rate_hz: u32) -> [f32; SPECTRUM_HARMONICS.len()] {
    let n = window.len() as f32;
    let mut magnitudes = [0.0f32; SPECTRUM_HARMONICS.len()];
    for (bin, harmonic) in SPECTRUM_HARMONICS.iter().enumerate() {
        let frequency = MAINS_HZ * *harmonic as f32;
        let coefficient =
            2.0 * cosf(2.0 * core::f32::consts::PI * frequency / sample_rate_hz as f32);
        let mut s1 = 0.0f32;
        let mut s2 = 0.0f32;
        for sample in window {
            let s0 = sample + s1 * coefficient - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = s1 * s1 + s2 * s2 - s1 * s2 * coefficient;
        magnitudes[bin] = sqrtf(power.max(0.0)) / n;
    }
    magnitudes
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use std::vec::Vec;

    const FS: u32 = 1600;

    /// A clean 2 A 50 Hz window starting mid-period (no sample sits exactly
    /// on a zero passage — an on-grid exact 0.0 kills strict-product counts).
    fn window(amplitude: f32) -> Vec<f32> {
        (0..128)
            .map(|t| {
                let ts = t as f32 / FS as f32;
                amplitude
                    * (2.0 * core::f32::consts::PI * MAINS_HZ * ts + core::f32::consts::FRAC_PI_8)
                        .sin()
            })
            .collect()
    }

    #[test]
    fn rms_of_clean_sine() {
        // RMS of a sine is amplitude / sqrt(2)
        let features = extract(&window(2.0), FS);
        assert!((features.rms - 2.0 / 2.0f32.sqrt()).abs() < 1e-3);
        assert!((features.peak - 2.0).abs() < 1e-3);
    }

    #[test]
    fn zero_crossings_count_periods() {
        // 128 samples @ 1.6 kHz = 4 periods of 50 Hz -> 8 crossings
        let features = extract(&window(1.0), FS);
        assert_eq!(features.zero_crossings, 8);
    }

    #[test]
    fn spectrum_finds_the_carrier() {
        let features = extract(&window(1.5), FS);
        // A pure sine at an exact bin: the fundamental bin reads A/2, the
        // harmonic bins only carry float32 recurrence noise.
        assert!(
            (features.spectrum[0] - 0.75).abs() < 1e-3,
            "got {}",
            features.spectrum[0]
        );
        assert!(features.spectrum[1] < 1e-3, "got {}", features.spectrum[1]);
        assert!(features.spectrum[2] < 1e-3, "got {}", features.spectrum[2]);
    }

    #[test]
    fn zero_window_is_neutral() {
        let features = extract(&[0.0; 128], FS);
        assert_eq!(features.rms, 0.0);
        assert_eq!(features.peak, 0.0);
        assert_eq!(features.zero_crossings, 0);
        assert_eq!(features.spectrum, [0.0; 3]);
    }
}
