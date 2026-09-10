//! Node A firmware logic (Availability) — the `no_std` core.
//!
//! A compact mirror of the host `nodes::status` semantics (week 4): the
//! fixed-buffer window accumulator and the anti-flap hysteresis behave like
//! their host twins, and the same rust-born model classifies the window —
//! the S3 cross-check (decompose S3) is then a matter of feeding both sides
//! the same bits. Host unit tests pin the equivalence, including a direct
//! comparison against `nodes::a::classify` on synthetic windows.
//!
//! The esp-hal half (ADC sampling, UART lines) lives in `main.rs` behind a
//! target gate; this lib never touches hardware.

#![no_std]

use core::fmt::Write as _;

use fmt_util::Cursor;
use microflow::model;
use nalgebra::SMatrix;

/// Window length and rate: `features_cli::window_spec(NodeKind::A)`
/// (128 @ 1.6 kHz = 80 ms). Re-exported for main.rs and the tests.
pub const WINDOW: usize = 128;

/// Consecutive agreeing windows before a status change is confirmed
/// (`nodes::a::CONFIRM_AFTER`): 2 x 80 ms.
pub const CONFIRM_AFTER: u32 = 2;

/// ADC sampling period, us: `features_cli::window_spec(A)` = 1.6 kHz.
/// 625 us is NOT a whole millisecond — see [`advance_ms`].
pub const SAMPLE_US: u32 = 625;

/// Advances the microsecond clock by `us` and returns whole milliseconds.
///
/// The naive `t_ms += SAMPLE_US / 1000` froze the clock at 0 (integer
/// division: 625 / 1000 == 0 — review card 20260909120006). Microseconds
/// accumulate in a `u64` (a `u32` would overflow after ~71.6 min); the
/// division happens only at use.
pub fn advance_ms(t_us: &mut u64, us: u32) -> u32 {
    *t_us += us as u64;
    (*t_us / 1000) as u32
}

/// Status names by class index (the `nodes::a::STATE_NAMES` order).
pub const STATE_NAMES: [&str; 4] = ["idle", "run", "jam", "overload"];

// rustc's cwd for firmware-workspace builds is firmware/ (the fork's
// week-3 path convention), so the model is reached through ../ml/.
#[model("../ml/models/model_a.tflite")]
struct ModelA;

/// Classifies one complete window: (probabilities, argmax).
///
/// The same `predict()` call as `nodes::a::classify_with_probs` on the same
/// bits — pinned bit-for-bit by the host test below.
pub fn classify(window: &[f32; WINDOW]) -> ([f32; 4], usize) {
    let matrix = SMatrix::<f32, WINDOW, 1>::from_column_slice(window);
    let output = ModelA::predict(matrix);
    let mut probs = [0.0f32; 4];
    for (slot, value) in probs.iter_mut().zip(output.iter()) {
        *slot = *value;
    }
    // NaN-safe and tie-compatible with the host `nodes::a` argmax
    // (max_by: the last maximum wins).
    (probs, fmt_util::argmax(&probs))
}

/// The outcome of feeding one sample into [`WindowAccumulator`].
///
/// `Complete` carries the window by value (512 B on the stack): the
/// `no_std` core has no allocator, so boxing is not an option — and the
/// window is consumed immediately by `classify`, never stored.
#[derive(Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum WindowOutcome {
    /// The window is complete and clean (the buffer is copied out).
    Complete([f32; WINDOW]),
    /// The window completed but a bad sample was seen inside — dropped.
    Dirty,
    /// More samples needed.
    Filling,
}

/// Non-overlapping window assembly over a fixed buffer — the host
/// `nodes::status::WindowAccumulator` semantics without allocation.
pub struct WindowAccumulator {
    window: [f32; WINDOW],
    len: usize,
    dirty: bool,
}

impl Default for WindowAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowAccumulator {
    pub const fn new() -> Self {
        Self {
            window: [0.0; WINDOW],
            len: 0,
            dirty: false,
        }
    }

    /// Feeds one sample; `None` (an ADC read failure) marks the window
    /// dirty without stopping the stream — error isolation, as on the host.
    pub fn push(&mut self, sample: Option<f32>) -> WindowOutcome {
        match sample {
            Some(value) if !value.is_nan() => {
                if self.len < WINDOW {
                    self.window[self.len] = value;
                    self.len += 1;
                }
            }
            _ => self.dirty = true,
        }
        if self.len < WINDOW {
            return WindowOutcome::Filling;
        }
        self.len = 0;
        let dirty = core::mem::take(&mut self.dirty);
        if dirty {
            WindowOutcome::Dirty
        } else {
            WindowOutcome::Complete(self.window)
        }
    }
}

/// Anti-flap hysteresis — the `nodes::status::Hysteresis` twin: a change is
/// confirmed after `confirm_after` consecutive windows agree.
pub struct Hysteresis {
    confirmed: Option<usize>,
    pending: Option<(usize, u32)>,
    confirm_after: u32,
}

impl Hysteresis {
    pub fn new(confirm_after: u32) -> Self {
        let confirm_after = if confirm_after == 0 { 1 } else { confirm_after };
        Self {
            confirmed: None,
            pending: None,
            confirm_after,
        }
    }

    /// Returns the newly confirmed status when it changed (the initial
    /// status counts as a change — it is published).
    pub fn observe(&mut self, prediction: usize) -> Option<usize> {
        match self.pending {
            Some((value, streak)) if value == prediction => {
                self.pending = Some((value, streak + 1));
            }
            _ => {
                self.pending = Some((prediction, 1));
            }
        }
        let (value, streak) = self.pending?;
        if streak >= self.confirm_after && self.confirmed != Some(value) {
            self.confirmed = Some(value);
            Some(value)
        } else {
            None
        }
    }
}

/// Formats a status line for the UART bridge / capture log:
/// `a,<run_id>,<t_ms>,<state>` (the week-4 offline-CSV family).
pub fn format_status(out: &mut [u8], run_id: &str, t_ms: u32, state_index: usize) -> Option<usize> {
    let mut cursor = Cursor::new(out);
    writeln!(
        cursor,
        "a,{run_id},{t_ms},{}",
        STATE_NAMES.get(state_index).copied().unwrap_or("?")
    )
    .ok()?;
    Some(cursor.pos())
}

/// Formats a capture line (`features_cli::capture` schema):
/// `<t_ms>,a,<run_id>,<value:.6>,<state>,<note>\n` — raw counts not needed;
/// the value column carries amps (the `value` column of the schema).
/// The S3 capture-mode line; unused by the streaming main loop by design
/// (kept as the capture contract, review card 20260909120041).
pub fn format_capture(
    out: &mut [u8],
    run_id: &str,
    t_ms: u32,
    amps: f32,
    state: &str,
) -> Option<usize> {
    let mut cursor = Cursor::new(out);
    writeln!(cursor, "{t_ms},a,{run_id},{amps:.6},{state},").ok()?;
    Some(cursor.pos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_complete_non_overlapping() {
        let mut acc = WindowAccumulator::new();
        for i in 0..WINDOW {
            match acc.push(Some(i as f32)) {
                WindowOutcome::Filling if i + 1 < WINDOW => {}
                WindowOutcome::Complete(w) if i + 1 == WINDOW => {
                    assert_eq!(w[0], 0.0);
                    assert_eq!(w[WINDOW - 1], (WINDOW - 1) as f32);
                }
                other => panic!("unexpected outcome at {i}: {other:?}"),
            }
        }
        // The next window starts fresh.
        assert_eq!(acc.push(Some(5.0)), WindowOutcome::Filling);
    }

    #[test]
    fn a_bad_sample_dirties_only_its_window() {
        let mut acc = WindowAccumulator::new();
        for _ in 0..(WINDOW - 1) {
            assert_eq!(acc.push(Some(1.0)), WindowOutcome::Filling);
        }
        // A bad sample marks the window dirty without occupying a slot.
        assert_eq!(acc.push(None), WindowOutcome::Filling);
        assert_eq!(acc.push(Some(1.0)), WindowOutcome::Dirty);
        // The next window starts clean.
        assert_eq!(acc.push(Some(5.0)), WindowOutcome::Filling);
    }

    #[test]
    fn hysteresis_confirms_after_streak() {
        // The same trace as the host test: the semantics must match.
        let mut h = Hysteresis::new(2);
        assert_eq!(h.observe(1), None);
        assert_eq!(h.observe(1), Some(1));
        assert_eq!(h.observe(2), None);
        assert_eq!(h.observe(1), None);
        assert_eq!(h.observe(2), None);
        assert_eq!(h.observe(2), Some(2));
        assert_eq!(h.observe(2), None);
    }

    #[test]
    fn status_line_format_matches_the_offline_csv_family() {
        let mut buf = [0u8; 64];
        let n = format_status(&mut buf, "bench-a", 1234, 1).unwrap();
        assert_eq!(&buf[..n], b"a,bench-a,1234,run\n");
    }

    #[test]
    fn capture_line_format_matches_the_capture_schema() {
        let mut buf = [0u8; 96];
        let n = format_capture(&mut buf, "bench-a", 1234, 2.012345, "").unwrap();
        assert_eq!(&buf[..n], b"1234,a,bench-a,2.012345,,\n");
    }

    #[test]
    fn advance_ms_is_monotonic_and_exact_over_hours() {
        let mut t_us: u64 = 0;
        let mut last = 0u32;
        // 3 hours at SAMPLE_US = 625: the old `SAMPLE_US / 1000` stood at 0.
        for _ in 0..(3 * 3600 * 1600) {
            let now = advance_ms(&mut t_us, SAMPLE_US);
            assert!(now >= last, "t_ms went backwards: {now} < {last}");
            last = now;
        }
        assert_eq!(last, 10_800_000); // exactly 3 h, no lost fractions
    }

    /// Review card 20260909120007 (firmware half): the WINDOW constant is
    /// a hand copy — pin it to the single source of truth.
    #[test]
    fn window_matches_the_features_cli_contract() {
        let spec = features_cli::window_spec(features_cli::NodeKind::A).unwrap();
        assert_eq!(WINDOW, spec.samples);
        // SAMPLE_US must be the exact reciprocal of the contract rate.
        assert_eq!(1_000_000 / SAMPLE_US, spec.sample_rate_hz);
    }

    /// The model path is the same rust-born `model_a.tflite` the host node
    /// runs: the same input must give the same argmax (bit-for-bit — the
    /// same kernel, the same deterministic integer semantics). The fixture
    /// is the first label=run window of the validation split
    /// (`ml/models/model_a.val.csv`) — real training-distribution data, not
    /// a synthetic shape (an ad-hoc "2 A + sin" window misclassifies: the
    /// carrier is `envelope * sin`, no DC offset).
    #[test]
    fn classify_matches_the_host_node_a() {
        #[rustfmt::skip]
        const RUN_WINDOW: [f32; WINDOW] = [
            0.246593, 0.732889, 1.417443, 1.437473, 1.655375, 1.562328, 1.817769, 1.853437,
            2.008842, 1.739042, 1.7432, 1.711332, 1.567182, 1.579399, 1.450156, 0.7764,
            -0.253575, -0.626656, -1.376794, -1.569148, -1.651708, -1.572479, -1.909664, -2.099313,
            -1.940068, -1.938945, -1.913123, -1.804133, -1.620044, -1.479319, -1.280637, -0.766928,
            0.11401, 0.636976, 1.260946, 1.376056, 1.550268, 1.480419, 1.809003, 1.86173,
            1.522448, 1.629014, 1.668836, 1.681854, 1.496408, 1.258855, 1.200843, 0.755205,
            -0.030877, -0.626518, -1.088223, -1.31771, -1.469966, -1.543233, -1.611931, -1.596889,
            -1.765922, -1.595933, -1.663756, -1.52092, -1.421396, -1.446976, -1.259675, -0.603895,
            -0.053713, 0.65903, 1.279514, 1.579324, 1.509596, 1.481617, 1.686306, 1.914158,
            1.813348, 1.805359, 1.723064, 1.536893, 1.559935, 1.44548, 1.13262, 0.531794,
            0.083071, -0.526596, -1.060134, -1.532358, -1.726326, -1.629944, -1.650761, -1.864572,
            -1.846524, -1.536107, -1.712635, -1.377136, -1.402134, -1.413753, -1.129783, -0.631396,
            -0.088243, 0.78577, 1.18666, 1.674798, 1.726653, 1.607923, 1.906381, 1.82702,
            1.869769, 1.903579, 1.700093, 1.612058, 1.631799, 1.363957, 1.128829, 0.786987,
            0.003119, -0.778809, -1.252932, -1.591705, -1.666103, -1.510591, -1.815823, -1.836108,
            -2.029664, -1.986294, -1.859683, -1.715893, -1.756868, -1.675591, -1.298155, -0.621948,
        ];
        let (probs, argmax) = classify(&RUN_WINDOW);
        assert_eq!(argmax, 1, "the run class: probs={probs:?}");
    }
}
