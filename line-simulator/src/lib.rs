//! Deterministic production-line simulator (OEE plan sections 3-4).
//!
//! Machine FSM `idle -> run/jam/overload` driven by a declarative scenario,
//! current-signal synthesis (50 Hz + harmonics + per-mode envelope +
//! amplitude drift + seeded noise; parameters live in the scenario), CSV
//! output "time, current, true mode".
//!
//! Determinism: one seed -> a bit-identical CSV (checked by a test).

pub mod dataset;
pub mod fsm;
pub mod scenario;
pub mod signal;
pub mod taps;

use rand::{rngs::StdRng, SeedableRng};

use crate::fsm::MachineState;
use crate::scenario::{Envelope, Noise, Signal};
use crate::signal::SignalGenerator;

/// One row of the simulator CSV output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Time since start, ms (step = 1 / SAMPLE_RATE_HZ).
    pub t_ms: u32,
    /// Current, A (synthetic signal).
    pub current_a: f32,
    /// True machine mode (ground truth for node A).
    pub state: MachineState,
}

/// One simulator run: seed -> a deterministic sample stream.
/// Mode changes come from scenario events via [`Simulator::apply`].
pub struct Simulator {
    rng: StdRng,
    state: MachineState,
    sample_index: u64,
    generator: SignalGenerator,
}

impl Simulator {
    pub fn new(seed: u64, signal: Signal) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
            state: MachineState::Idle,
            sample_index: 0,
            generator: SignalGenerator::new(signal),
        }
    }

    /// Current mode (ground truth).
    pub fn state(&self) -> MachineState {
        self.state
    }

    /// Next sample time, ms (no generation — for event scheduling).
    pub fn next_t_ms(&self) -> u32 {
        ((self.sample_index as f32 / scenario::SAMPLE_RATE_HZ) * 1000.0) as u32
    }

    /// Generates the next signal sample for the current mode.
    pub fn next_sample(&mut self, envelope: &Envelope, noise: &Noise) -> Sample {
        let t_ms = self.next_t_ms();
        let t = self.sample_index as f32 / scenario::SAMPLE_RATE_HZ;
        self.sample_index += 1;
        let current = self
            .generator
            .sample(t, self.state, envelope, noise, &mut self.rng);
        Sample {
            t_ms,
            current_a: current,
            state: self.state,
        }
    }

    /// Applies a scenario event (mode change).
    pub fn apply(&mut self, event: MachineState) {
        self.state = event;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two runs with the same seed -> bit-identical samples (gate D6).
    #[test]
    fn same_seed_same_bits() {
        let envelope = Envelope {
            idle: 0.4,
            run: 2.0,
            jam: 3.2,
            overload: 4.5,
        };
        let noise = Noise { sigma_a: 0.1 };
        let signal = Signal::default();
        let events = [
            MachineState::Run,
            MachineState::Jam,
            MachineState::Run,
            MachineState::Idle,
        ];
        let run = |seed: u64| {
            let mut sim = Simulator::new(seed, signal);
            (0..4_000)
                .map(|i| {
                    if i % 1_000 == 0 {
                        sim.apply(events[(i / 1_000) as usize % events.len()]);
                    }
                    let s = sim.next_sample(&envelope, &noise);
                    (s.t_ms, s.current_a.to_bits(), s.state)
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run(42), run(42));
        assert_ne!(run(42), run(43));
    }
}
