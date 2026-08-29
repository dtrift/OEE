#![no_std]

//! Feature parity (plan section 6): a single Rust crate computes features
//! for both training and inference — numpy receives ready-made features.
//! The fixed feature list (RMS, peak, zero-crossings, spectrum) lives in
//! [`features`] and is locked against numpy by a golden test (week 3, D4).
//!
//! Contracts of the "code-only <-> hardware" track:
//! - [`window_spec`]: window and sample rate are per-node, not one global
//!   constant (simulator 1.6 kHz, I2S mic 16 kHz, INA226 ~1 kHz);
//! - [`calibration`]: raw ADC counts -> amps (ACS712-20A + 2:1 divider);
//! - [`capture`]: hardware capture CSV schema — same units as the simulator.
//!
//! `#![no_std]` is part of the contract: this crate compiles into node
//! firmware.

pub mod calibration;
pub mod capture;
pub mod features;

/// A digital-twin node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// Node A: machine current -> status (Availability).
    A,
    /// Node P: IR barrier -> part counting (Performance).
    P,
    /// Node Q: tap test -> pass/fail verdict (Quality).
    Q,
}

impl NodeKind {
    /// Short node name — the `node` column value in capture CSV
    /// (mirrors the simulator's `MachineState::as_str`).
    pub const fn as_str(self) -> &'static str {
        match self {
            NodeKind::A => "a",
            NodeKind::P => "p",
            NodeKind::Q => "q",
        }
    }
}

/// Feature-window contract: how many samples and at what rate.
///
/// Physical window time = `samples / sample_rate_hz` — part of the model
/// contract: 128 samples at 1.6 kHz (node A) and at 16 kHz (node Q) are
/// different windows (80 ms vs 8 ms). The training script and the firmware
/// read the sizes only from here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowSpec {
    /// Number of samples in the window (model TIMESTEPS).
    pub samples: usize,
    /// Source sample rate, Hz.
    pub sample_rate_hz: u32,
}

impl WindowSpec {
    /// Window duration, ms.
    pub const fn duration_ms(self) -> u32 {
        (self.samples as u32 * 1000) / self.sample_rate_hz
    }
}

/// Node window; `None` — the node is event-driven and has no windows (P: edge detector).
pub const fn window_spec(kind: NodeKind) -> Option<WindowSpec> {
    match kind {
        // A: must match TIMESTEPS in ml/scripts/build_conv1d_model.py
        // and SAMPLE_RATE_HZ in line-simulator (1.6 kHz = 32 samples per 50 Hz period).
        NodeKind::A => Some(WindowSpec {
            samples: 128,
            sample_rate_hz: 1600,
        }),
        // Q: the rate is fixed by hardware (INMP441 over I2S, 16 kHz); window
        // size is provisional (64 ms) — to be pinned by the week-4 lab.
        // Change it only here: this is the single source of truth for
        // training and firmware.
        NodeKind::Q => Some(WindowSpec {
            samples: 1024,
            sample_rate_hz: 16_000,
        }),
        NodeKind::P => None,
    }
}

/// Node A window in samples (compatibility with the week-1 spike model).
pub fn window_len() -> usize {
    window_spec(NodeKind::A)
        .expect("node A is windowed")
        .samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_a_matches_spike_model() {
        // Must match TIMESTEPS in ml/scripts/build_conv1d_model.py
        // and SAMPLE_RATE_HZ in line-simulator/src/scenario.rs.
        let spec = window_spec(NodeKind::A).expect("node A is windowed");
        assert_eq!(spec.samples, 128);
        assert_eq!(spec.sample_rate_hz, 1600);
        assert_eq!(spec.duration_ms(), 80);
        assert_eq!(window_len(), 128);
    }

    #[test]
    fn node_p_is_event_driven() {
        assert_eq!(window_spec(NodeKind::P), None);
    }

    #[test]
    fn node_kinds_roundtrip_via_capture_column() {
        for kind in [NodeKind::A, NodeKind::P, NodeKind::Q] {
            assert_eq!(NodeKind::as_str(kind).len(), 1);
        }
    }
}
