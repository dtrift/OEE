//! FSM станка: `idle → run → jam/overload` (разд. 4 плана).
//!
//! Переходы управляются декларативным сценарием (список событий с моментами
//! времени) — сценарий же служит ground truth для оценки качества узлов.

use serde::Deserialize;

/// Режим станка. Порядок вариантов фиксирован: по нему сериализуется CSV.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MachineState {
    Idle,
    Run,
    Jam,
    Overload,
}

impl MachineState {
    /// Стабильное строковое имя для CSV/логов (ground truth-колонка).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Run => "run",
            Self::Jam => "jam",
            Self::Overload => "overload",
        }
    }
}

/// Событие сценария: в момент `t_ms` станок переходит в `state`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ScenarioEvent {
    pub t_ms: u32,
    pub state: MachineState,
}

impl ScenarioEvent {
    /// Валидация: события должны быть строго по возрастанию времени
    /// (иначе сценарий физически не воспроизводим).
    pub fn validate(events: &[ScenarioEvent]) -> Result<(), String> {
        let Some(first) = events.first() else {
            return Ok(());
        };
        if first.t_ms < 1 {
            return Err(
                "первое событие раньше t=1 мс (стартовое состояние задайте как idle)".into(),
            );
        }
        events.windows(2).try_for_each(|pair| {
            if pair[0].t_ms >= pair[1].t_ms {
                Err(format!(
                    "события не по возрастанию времени: {} мс затем {} мс",
                    pair[0].t_ms, pair[1].t_ms
                ))
            } else {
                Ok(())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_names_are_stable() {
        assert_eq!(MachineState::Idle.as_str(), "idle");
        assert_eq!(MachineState::Overload.as_str(), "overload");
    }

    #[test]
    fn events_must_be_sorted() {
        let events = [
            ScenarioEvent {
                t_ms: 500,
                state: MachineState::Run,
            },
            ScenarioEvent {
                t_ms: 200,
                state: MachineState::Jam,
            },
        ];
        assert!(ScenarioEvent::validate(&events).is_err());
    }

    #[test]
    fn empty_scenario_is_valid() {
        assert!(ScenarioEvent::validate(&[]).is_ok());
    }
}
