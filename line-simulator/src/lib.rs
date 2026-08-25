//! Детерминированный симулятор производственной линии (разд. 3–4 плана OEE).
//!
//! FSM станка `idle → run/jam/overload` по декларативному сценарию,
//! синтез сигнала тока (50 Гц + гармоники + огибающая по режиму + дрейф
//! амплитуды + шум по seed; параметры — в сценарии), вывод CSV
//! «время, ток, истинный режим».
//!
//! Детерминизм: один seed → побитово одинаковый CSV (проверяется тестом).

pub mod fsm;
pub mod scenario;
pub mod signal;

use rand::{rngs::StdRng, SeedableRng};

use crate::fsm::MachineState;
use crate::scenario::{Envelope, Noise, Signal};
use crate::signal::SignalGenerator;

/// Строка CSV-вывода симулятора.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Время от старта, мс (шаг = 1 / SAMPLE_RATE_HZ).
    pub t_ms: u32,
    /// Ток, А (синтетический сигнал).
    pub current_a: f32,
    /// Истинный режим станка (ground truth для узла A).
    pub state: MachineState,
}

/// Прогон симулятора: seed → детерминированный поток сэмплов.
/// Смена режима — через [`Simulator::apply`] по событиям сценария.
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

    /// Текущий режим (ground truth).
    pub fn state(&self) -> MachineState {
        self.state
    }

    /// Время следующего сэмпла, мс (без генерации — для планирования событий).
    pub fn next_t_ms(&self) -> u32 {
        ((self.sample_index as f32 / scenario::SAMPLE_RATE_HZ) * 1000.0) as u32
    }

    /// Генерирует следующий сэмпл сигнала для текущего режима.
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

    /// Применяет событие сценария (смена режима).
    pub fn apply(&mut self, event: MachineState) {
        self.state = event;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Два прогона с одним seed → побитово одинаковые сэмплы (гейт Д6).
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
