//! Синтез сигнала тока: несущая 50 Гц + гармоники + огибающая по режиму +
//! шум (seeded RNG) — разд. 4 плана.
//!
//! Детерминизм: при одном seed последовательность вызовов `synthesize`
//! даёт побитово одинаковый сигнал.

use rand::Rng;
use rand_distr::Normal;

use crate::fsm::MachineState;
use crate::scenario::{Envelope, Noise};

/// Частота сети, Гц.
pub const MAINS_HZ: f32 = 50.0;

/// Синтез одного отсчёта тока в момент `t` (с) для режима `state`.
///
/// `envelope` задаёт амплитуду по режиму; шум — гауссов с СКО `noise.sigma_a`.
pub fn synthesize(
    t: f32,
    state: MachineState,
    envelope: &Envelope,
    noise: &Noise,
    rng: &mut impl Rng,
) -> f32 {
    let amplitude = envelope.by_state(state);
    let mains = (2.0 * core::f32::consts::PI * MAINS_HZ * t).sin();
    let third = (2.0 * core::f32::consts::PI * 3.0 * MAINS_HZ * t).sin();
    let fifth = (2.0 * core::f32::consts::PI * 5.0 * MAINS_HZ * t).sin();
    let noise: f32 = rng.sample(Normal::new(0.0, noise.sigma_a).expect("sigma > 0"));
    amplitude * (mains + 0.15 * third + 0.07 * fifth) + noise
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn env() -> (Envelope, Noise) {
        (
            Envelope {
                idle: 0.4,
                run: 2.0,
                jam: 3.2,
                overload: 4.5,
            },
            Noise { sigma_a: 0.0 },
        )
    }

    #[test]
    fn deterministic_given_seed() {
        let (envelope, noise) = env();
        let mut rng1 = StdRng::seed_from_u64(42);
        let mut rng2 = StdRng::seed_from_u64(42);
        for i in 0..1000 {
            let t = i as f32 / 1600.0;
            let a = synthesize(t, MachineState::Run, &envelope, &noise, &mut rng1);
            let b = synthesize(t, MachineState::Run, &envelope, &noise, &mut rng2);
            assert_eq!(a.to_bits(), b.to_bits(), "sample {i}");
        }
    }

    #[test]
    fn modes_differ_in_amplitude() {
        let (envelope, noise) = env();
        let mut rng = StdRng::seed_from_u64(1);
        // Пик за полный период больше для jam, чем для run (без шума).
        let peak = |state, rng: &mut StdRng| {
            (0..1600 / 50)
                .map(|i| {
                    let t = i as f32 / crate::scenario::SAMPLE_RATE_HZ;
                    synthesize(t, state, &envelope, &noise, rng).abs()
                })
                .fold(0.0f32, f32::max)
        };
        let run = peak(MachineState::Run, &mut rng);
        let jam = peak(MachineState::Jam, &mut rng);
        assert!(jam > run * 1.5, "jam={run}, run={jam}");
    }
}
