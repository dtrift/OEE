//! Run scenario: a declarative TOML file (also the ground truth).

use serde::Deserialize;

use crate::fsm::{MachineState, ScenarioEvent};

/// Current-signal sample rate, Hz.
/// 1.6 kHz = 32 samples per 50 Hz period — enough for 3 harmonics.
pub const SAMPLE_RATE_HZ: f32 = 1600.0;

/// Default run duration, ms.
pub const DEFAULT_DURATION_MS: u32 = 60_000;

/// Signal noise parameters (level is a fraction of the envelope amplitude).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Noise {
    /// Noise standard deviation, A.
    pub sigma_a: f32,
}

/// Current-signal shape: carrier harmonics and amplitude drift (week-2 item D5).
///
/// Harmonic amplitudes are fractions of the fundamental; drift is the
/// standard deviation of the amplitude multiplier, refreshed once per mains
/// period (20 ms).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Signal {
    /// 3rd harmonic amplitude (fraction of the fundamental).
    pub third: f32,
    /// 5th harmonic amplitude (fraction of the fundamental).
    pub fifth: f32,
    /// Per-period amplitude drift standard deviation (fraction of nominal).
    pub drift_sigma: f32,
}

impl Default for Signal {
    fn default() -> Self {
        Self {
            third: 0.15,
            fifth: 0.07,
            drift_sigma: 0.05,
        }
    }
}

/// Per-mode current amplitude envelope, A.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub idle: f32,
    pub run: f32,
    pub jam: f32,
    pub overload: f32,
}

impl Envelope {
    pub fn by_state(&self, state: MachineState) -> f32 {
        match state {
            MachineState::Idle => self.idle,
            MachineState::Run => self.run,
            MachineState::Jam => self.jam,
            MachineState::Overload => self.overload,
        }
    }
}

/// Simulator run scenario.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Duration, ms.
    pub duration_ms: u32,
    /// Mode-change events (in increasing t_ms).
    pub events: Vec<ScenarioEvent>,
    /// Envelope amplitudes per mode.
    #[serde(default = "default_envelope")]
    pub envelope: Envelope,
    /// Noise.
    #[serde(default = "default_noise")]
    pub noise: Noise,
    /// Signal shape: harmonics and amplitude drift.
    #[serde(default)]
    pub signal: Signal,
}

fn default_envelope() -> Envelope {
    Envelope {
        idle: 0.4,
        run: 2.0,
        jam: 3.2,
        overload: 4.5,
    }
}

fn default_noise() -> Noise {
    Noise { sigma_a: 0.1 }
}

impl Scenario {
    /// Parses a scenario from TOML text with event validation.
    pub fn parse(text: &str) -> Result<Self, String> {
        let scenario: Self = toml::from_str(text).map_err(|e| format!("scenario: {e}"))?;
        ScenarioEvent::validate(&scenario.events)?;
        Ok(scenario)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = r#"
duration_ms = 1000
[[events]]
t_ms = 100
state = "Run"
[[events]]
t_ms = 600
state = "Jam"
"#;

    #[test]
    fn parses_base_scenario() {
        let s = Scenario::parse(BASE).expect("parse");
        assert_eq!(s.duration_ms, 1000);
        assert_eq!(s.events.len(), 2);
        assert_eq!(s.events[0].state, MachineState::Run);
        assert_eq!(s.envelope.run, 2.0); // default
        assert_eq!(s.signal.third, 0.15); // default
        assert_eq!(s.signal.drift_sigma, 0.05);
    }

    #[test]
    fn parses_signal_section() {
        let text = "duration_ms = 1\nevents = []\n[signal]\nfifth = 0.02\n";
        let s = Scenario::parse(text).expect("parse");
        assert_eq!(s.signal.third, 0.15); // unset — default
        assert_eq!(s.signal.fifth, 0.02);
        assert_eq!(s.signal.drift_sigma, 0.05);
    }

    #[test]
    fn rejects_unsorted_events() {
        let bad = r#"
duration_ms = 1000
[[events]]
t_ms = 600
state = "Jam"
[[events]]
t_ms = 100
state = "Run"
"#;
        assert!(Scenario::parse(bad).is_err());
    }

    #[test]
    fn rejects_unknown_fields() {
        let bad = "duration_ms = 1\nunknown_key = 2\n";
        assert!(Scenario::parse(bad).is_err());
    }
}
