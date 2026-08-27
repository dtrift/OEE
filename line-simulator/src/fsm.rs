//! Machine FSM: `idle -> run -> jam/overload` (plan section 4).
//!
//! Transitions are driven by a declarative scenario (a list of events with
//! time points) — the same scenario serves as ground truth for evaluating
//! node quality.

use serde::Deserialize;

/// Machine mode. Variant order is fixed: CSV serialization follows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MachineState {
    Idle,
    Run,
    Jam,
    Overload,
}

impl MachineState {
    /// Stable string name for CSV/logs (ground-truth column).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Run => "run",
            Self::Jam => "jam",
            Self::Overload => "overload",
        }
    }
}

/// Scenario event: at time `t_ms` the machine switches to `state`.
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct ScenarioEvent {
    pub t_ms: u32,
    pub state: MachineState,
}

impl ScenarioEvent {
    /// Validation: events must be strictly time-increasing
    /// (otherwise the scenario is physically unreproducible).
    pub fn validate(events: &[ScenarioEvent]) -> Result<(), String> {
        let Some(first) = events.first() else {
            return Ok(());
        };
        if first.t_ms < 1 {
            return Err("first event earlier than t=1 ms (set the initial state as idle)".into());
        }
        events.windows(2).try_for_each(|pair| {
            if pair[0].t_ms >= pair[1].t_ms {
                Err(format!(
                    "events not in increasing time order: {} ms then {} ms",
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
