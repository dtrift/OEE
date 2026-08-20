//! Сценарий прогона: декларативный TOML-файл (он же ground truth).

use serde::Deserialize;

use crate::fsm::{MachineState, ScenarioEvent};

/// Частота дискретизации сигнала тока, Гц.
/// 1.6 кГц = 32 отсчёта на период 50 Гц — достаточно для 3 гармоник.
pub const SAMPLE_RATE_HZ: f32 = 1600.0;

/// Длительность прогона по умолчанию, мс.
pub const DEFAULT_DURATION_MS: u32 = 60_000;

/// Параметры шума сигнала (уровень — доля амплитуды огибающей).
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Noise {
    /// СКО шума, А.
    pub sigma_a: f32,
}

/// Огибающая амплитуды тока по режимам, А.
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

/// Сценарий прогона симулятора.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Длительность, мс.
    pub duration_ms: u32,
    /// События смены режима (по возрастанию t_ms).
    pub events: Vec<ScenarioEvent>,
    /// Амплитуды огибающей по режимам.
    #[serde(default = "default_envelope")]
    pub envelope: Envelope,
    /// Шум.
    #[serde(default = "default_noise")]
    pub noise: Noise,
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
    /// Парсит сценарий из TOML-текста с валидацией событий.
    pub fn parse(text: &str) -> Result<Self, String> {
        let scenario: Self = toml::from_str(text).map_err(|e| format!("сценарий: {e}"))?;
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
        assert_eq!(s.envelope.run, 2.0); // дефолт
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
