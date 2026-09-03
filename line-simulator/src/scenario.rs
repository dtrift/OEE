//! Run scenario: a declarative TOML file (also the ground truth).

use serde::Deserialize;

use crate::fsm::{MachineState, ScenarioEvent};

/// Current-signal sample rate, Hz.
/// 1.6 kHz = 32 samples per 50 Hz period — enough for 3 harmonics.
pub const SAMPLE_RATE_HZ: f32 = 1600.0;

/// Default run duration, ms.
pub const DEFAULT_DURATION_MS: u32 = 60_000;

/// Tap-test channel parameters (week 4, D3): every machined part gets a
/// tap; the ring sound separates good parts from cracked ones. Defaults are
/// the "week-4 lab" baseline; scenario files override selectively.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Taps {
    /// Part period (one tap per part), ms.
    pub period_ms: u32,
    /// Probability that a part is cracked.
    pub crack_probability: f32,
    /// Good part: ring frequency, Hz.
    pub good_freq_hz: f32,
    /// Cracked part: lower ring frequency, Hz.
    pub cracked_freq_hz: f32,
    /// Good part decay time constant, ms.
    pub good_tau_ms: f32,
    /// Cracked part decay (faster), ms.
    pub cracked_tau_ms: f32,
    /// Tap amplitude (relative units).
    pub amplitude: f32,
    /// Amplitude wander per tap (fraction of nominal, seeded).
    pub amp_jitter: f32,
    /// Frequency wander per tap (fraction, seeded).
    pub freq_jitter: f32,
    /// Background noise sigma (relative units).
    pub noise_sigma: f32,
    /// Extra noise multiplier for cracked parts (the rattle).
    pub crack_noise_boost: f32,
}

impl Default for Taps {
    fn default() -> Self {
        Self {
            period_ms: 400,
            crack_probability: 0.25,
            good_freq_hz: 2400.0,
            cracked_freq_hz: 1500.0,
            good_tau_ms: 14.0,
            cracked_tau_ms: 6.0,
            amplitude: 0.8,
            amp_jitter: 0.15,
            freq_jitter: 0.08,
            noise_sigma: 0.01,
            crack_noise_boost: 4.0,
        }
    }
}

impl Taps {
    /// Validation: physically meaningful values only (the scenario is the
    /// ground truth — nonsense here poisons every consumer downstream).
    pub fn validate(&self) -> Result<(), String> {
        if self.period_ms == 0 {
            return Err("taps.period_ms must be positive".into());
        }
        if !(0.0..=1.0).contains(&self.crack_probability) {
            return Err("taps.crack_probability must be within [0, 1]".into());
        }
        for (name, value) in [
            ("good_freq_hz", self.good_freq_hz),
            ("cracked_freq_hz", self.cracked_freq_hz),
            ("good_tau_ms", self.good_tau_ms),
            ("cracked_tau_ms", self.cracked_tau_ms),
            ("amplitude", self.amplitude),
        ] {
            if value <= 0.0 {
                return Err(format!("taps.{name} must be positive"));
            }
        }
        let nyquist = crate::taps::TAP_SAMPLE_RATE_HZ as f32 / 2.0;
        if self.good_freq_hz >= nyquist || self.cracked_freq_hz >= nyquist {
            return Err(format!(
                "taps frequencies must stay under {nyquist} Hz (Nyquist)"
            ));
        }
        Ok(())
    }
}

/// Conveyor-belt channel parameters (week 5, D1): parts pass an IR barrier
/// while the machine runs — the raw material for node P (Performance).
/// Defaults match the nominal line cadence (one part per 400 ms, the same
/// period as the taps channel: both sample the same nominal production
/// rate through independent RNG streams).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Belt {
    /// Nominal part interval (one slot every period), ms.
    pub period_ms: u32,
    /// Pass-time jitter around the slot (fraction of the period, seeded).
    pub jitter: f32,
    /// Probability that a part re-triggers the barrier (the anti-double-
    /// count case: two pulses, still one part).
    pub double_probability: f32,
    /// Probability that a slot passes with no part at all (no IR events,
    /// not in the truth either).
    pub skip_probability: f32,
    /// IR-barrier blocked time per pulse, ms.
    pub pulse_ms: u32,
    /// Gap between the two pulses of a double, ms.
    pub double_gap_ms: u32,
}

impl Default for Belt {
    fn default() -> Self {
        Self {
            period_ms: 400,
            jitter: 0.15,
            double_probability: 0.1,
            skip_probability: 0.05,
            pulse_ms: 30,
            double_gap_ms: 40,
        }
    }
}

impl Belt {
    /// Validation: physically meaningful values only, and the pulses of a
    /// double must both fit strictly inside one nominal slot (they must not
    /// collide with the next part, whose earliest pass is
    /// `period_ms * (1 - jitter)` after the previous slot).
    pub fn validate(&self) -> Result<(), String> {
        if self.period_ms == 0 {
            return Err("belt.period_ms must be positive".into());
        }
        if !(0.0..=0.4).contains(&self.jitter) {
            return Err("belt.jitter must be within [0, 0.4]".into());
        }
        for (name, value) in [
            ("double_probability", self.double_probability),
            ("skip_probability", self.skip_probability),
        ] {
            if !(0.0..=1.0).contains(&value) {
                return Err(format!("belt.{name} must be within [0, 1]"));
            }
        }
        if self.pulse_ms == 0 {
            return Err("belt.pulse_ms must be positive".into());
        }
        if !(1..=10_000).contains(&self.double_gap_ms) {
            return Err("belt.double_gap_ms must be within [1, 10000]".into());
        }
        let pulses_span = 2.0 * self.pulse_ms as f32 + self.double_gap_ms as f32;
        let earliest_next = self.period_ms as f32 * (1.0 - self.jitter);
        if pulses_span >= earliest_next {
            return Err(format!(
                "belt: a double spans {pulses_span:.0} ms, parts can pass as close as \
                 {earliest_next:.0} ms — pulses would collide; widen period_ms or \
                 shrink pulse_ms/double_gap_ms"
            ));
        }
        Ok(())
    }
}

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
    /// Tap-test channel (week 4). The section is optional: without it the
    /// defaults apply and taps are only emitted when the CLI asks for them.
    #[serde(default)]
    pub taps: Taps,
    /// Conveyor-belt channel (week 5). Optional like `[taps]`: the defaults
    /// apply and events are only emitted when the CLI asks for them.
    #[serde(default)]
    pub belt: Belt,
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
        scenario.taps.validate()?;
        scenario.belt.validate()?;
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
    fn parses_taps_section() {
        let text =
            "duration_ms = 1\nevents = []\n[taps]\ncrack_probability = 0.4\nperiod_ms = 250\n";
        let s = Scenario::parse(text).expect("parse");
        assert_eq!(s.taps.period_ms, 250);
        assert_eq!(s.taps.crack_probability, 0.4);
        assert_eq!(s.taps.good_freq_hz, Taps::default().good_freq_hz); // unset — default
    }

    #[test]
    fn rejects_bad_taps() {
        let bad = "duration_ms = 1\nevents = []\n[taps]\ncrack_probability = 1.5\n";
        assert!(Scenario::parse(bad).is_err());
        let nyquist = "duration_ms = 1\nevents = []\n[taps]\ngood_freq_hz = 9000.0\n";
        assert!(Scenario::parse(nyquist).is_err());
    }

    #[test]
    fn parses_belt_section() {
        let text = "duration_ms = 1\nevents = []\n[belt]\nperiod_ms = 520\nbogus = 1\n";
        assert!(
            Scenario::parse(text).is_err(),
            "unknown fields are rejected"
        );
        let text = "duration_ms = 1\nevents = []\n[belt]\nperiod_ms = 520\njitter = 0.2\n";
        let s = Scenario::parse(text).expect("parse");
        assert_eq!(s.belt.period_ms, 520);
        assert_eq!(
            s.belt.double_probability,
            Belt::default().double_probability
        );
    }

    #[test]
    fn rejects_bad_belt() {
        let bad = "duration_ms = 1\nevents = []\n[belt]\njitter = 0.9\n";
        assert!(Scenario::parse(bad).is_err());
        // Pulses of a double wider than the shortest part interval: reject.
        let collide = "duration_ms = 1\nevents = []\n[belt]\nperiod_ms = 100\njitter = 0.0\n";
        assert!(Scenario::parse(collide).is_err());
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
