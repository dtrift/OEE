//! Синтез сигнала тока: несущая 50 Гц + гармоники + огибающая по режиму +
//! медленный дрейф амплитуды + шум (seeded RNG) — разд. 4 плана.
//!
//! Детерминизм: при одном seed последовательность вызовов [`SignalGenerator::sample`]
//! (t строго возрастает) даёт побитово одинаковый сигнал.

use rand::Rng;
use rand_distr::Normal;

use crate::fsm::MachineState;
use crate::scenario::{Envelope, Noise, Signal};

/// Частота сети, Гц.
pub const MAINS_HZ: f32 = 50.0;

/// Генератор сигнала тока: хранит параметры формы и текущий дрейф амплитуды.
///
/// Дрейф — множитель амплитуды, обновляемый раз за период сети (20 мс):
/// медленное блуждание вокруг номинала огибающей. Именно оно не даёт классам
/// режимов стать «идеально разделимыми» (риск разд. 11 плана).
pub struct SignalGenerator {
    signal: Signal,
    /// Текущий множитель амплитуды (1.0 = номинал).
    drift: f32,
    /// Индекс последнего обслуженного периода сети (u64::MAX — ещё ни одного).
    last_period: u64,
}

impl SignalGenerator {
    pub fn new(signal: Signal) -> Self {
        Self {
            signal,
            drift: 1.0,
            last_period: u64::MAX,
        }
    }

    /// Синтез одного отсчёта тока в момент `t` (с) для режима `state`.
    ///
    /// Контракт: `t` строго возрастает между вызовами (поток симулятора);
    /// при нарушении дрейф перестаёт обновляться, но детерминизм прогона
    /// не страдает — он и так обеспечивается последовательным потоком.
    pub fn sample(
        &mut self,
        t: f32,
        state: MachineState,
        envelope: &Envelope,
        noise: &Noise,
        rng: &mut impl Rng,
    ) -> f32 {
        let period = (t * MAINS_HZ) as u64;
        if period != self.last_period {
            self.last_period = period;
            let wander = rng
                .sample(Normal::new(0.0, self.signal.drift_sigma).expect("sigma >= 0"))
                .clamp(
                    -3.0 * self.signal.drift_sigma,
                    3.0 * self.signal.drift_sigma,
                );
            self.drift = 1.0 + wander;
        }
        let amplitude = envelope.by_state(state) * self.drift;
        let mains = (2.0 * core::f32::consts::PI * MAINS_HZ * t).sin();
        let third = (2.0 * core::f32::consts::PI * 3.0 * MAINS_HZ * t).sin();
        let fifth = (2.0 * core::f32::consts::PI * 5.0 * MAINS_HZ * t).sin();
        let noise: f32 = rng.sample(Normal::new(0.0, noise.sigma_a).expect("sigma >= 0"));
        amplitude * (mains + self.signal.third * third + self.signal.fifth * fifth) + noise
    }
}

/// RMS окна отсчётов — грубая фича разделимости режимов (разд. 4, 10 плана).
pub fn window_rms(window: &[f32]) -> f32 {
    let sum = window.iter().map(|s| s * s).sum::<f32>();
    (sum / window.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario::SAMPLE_RATE_HZ;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    fn env() -> (Envelope, Noise, Signal) {
        (
            Envelope {
                idle: 0.4,
                run: 2.0,
                jam: 3.2,
                overload: 4.5,
            },
            Noise { sigma_a: 0.1 },
            Signal {
                third: 0.15,
                fifth: 0.07,
                drift_sigma: 0.05,
            },
        )
    }

    /// Один seed → побитово одинаковый сигнал (гейт недели 1, сохранён).
    #[test]
    fn deterministic_given_seed() {
        let (envelope, noise, signal) = env();
        let gen = |seed: u64| {
            let mut generator = SignalGenerator::new(signal);
            let mut rng = StdRng::seed_from_u64(seed);
            (0..1000)
                .map(|i| {
                    let t = i as f32 / 1600.0;
                    generator.sample(t, MachineState::Run, &envelope, &noise, &mut rng)
                })
                .collect::<Vec<_>>()
        };
        let a = gen(42);
        let b = gen(42);
        for (i, (x, y)) in a.iter().zip(&b).enumerate() {
            assert_eq!(x.to_bits(), y.to_bits(), "sample {i}");
        }
        assert_ne!(a, gen(43));
    }

    /// Дрейф амплитуды: пики разных периодов внутри одного режима различаются.
    #[test]
    fn drift_varies_amplitude_within_mode() {
        let (envelope, noise, signal) = env();
        let mut generator = SignalGenerator::new(signal);
        let mut rng = StdRng::seed_from_u64(7);
        let mut peak_amplitudes = Vec::new();
        for period in 0..20u64 {
            let mut peak = 0.0f32;
            for i in 0..32 {
                let t = (period * 32 + i) as f32 / SAMPLE_RATE_HZ;
                let sample = generator.sample(t, MachineState::Run, &envelope, &noise, &mut rng);
                peak = peak.max(sample.abs());
            }
            peak_amplitudes.push(peak);
        }
        let min = peak_amplitudes.iter().cloned().fold(f32::MAX, f32::min);
        let max = peak_amplitudes.iter().cloned().fold(f32::MIN, f32::max);
        assert!(
            max - min > 0.01,
            "амплитуда не гуляет: min={min}, max={max}"
        );
    }

    /// Разделимость режимов: средние RMS окон (128 отсчётов — будущее окно
    /// CNN) различаются между режимами, но окна внутри режима неодинаковы —
    /// «различимо, но не идеально» (разд. 4 плана).
    #[test]
    fn rms_windows_separate_modes_but_vary_within() {
        let (envelope, noise, signal) = env();
        let window = |state: MachineState, seed: u64| -> Vec<f32> {
            let mut generator = SignalGenerator::new(signal);
            let mut rng = StdRng::seed_from_u64(seed);
            (0..64) // 64 окна по 128 отсчётов ≈ 5.1 с
                .map(|w| {
                    window_rms(
                        &(0..128)
                            .map(|i| {
                                let t = (w * 128 + i) as f32 / SAMPLE_RATE_HZ;
                                generator.sample(t, state, &envelope, &noise, &mut rng)
                            })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect()
        };
        let run = window(MachineState::Run, 42);
        let jam = window(MachineState::Jam, 42);
        let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
        let mean_run = mean(&run);
        let mean_jam = mean(&jam);
        assert!(
            mean_jam > mean_run * 1.4,
            "run vs jam: средние RMS не различаются: run={mean_run}, jam={mean_jam}"
        );
        let spread = |v: &[f32], m: f32| {
            (v.iter().map(|x| (x - m) * (x - m)).sum::<f32>() / v.len() as f32).sqrt()
        };
        assert!(
            spread(&run, mean_run) > 1e-3,
            "окна run одинаковы — классы слишком чистые"
        );
        assert!(
            spread(&jam, mean_jam) > 1e-3,
            "окна jam одинаковы — классы слишком чистые"
        );
    }

    /// Режимы различаются по пиковой амплитуде (сохранённый тест недели 1,
    /// устойчив к дрейфу: пик берётся за 20 периодов, rng у режимов независимые).
    #[test]
    fn modes_differ_in_amplitude() {
        let (envelope, noise, signal) = env();
        let peak = |state: MachineState| {
            let mut generator = SignalGenerator::new(signal);
            let mut rng = StdRng::seed_from_u64(1);
            (0..20 * 32)
                .map(|i| {
                    let t = i as f32 / SAMPLE_RATE_HZ;
                    generator
                        .sample(t, state, &envelope, &noise, &mut rng)
                        .abs()
                })
                .fold(0.0f32, f32::max)
        };
        let run = peak(MachineState::Run);
        let jam = peak(MachineState::Jam);
        assert!(jam > run * 1.5, "jam={run}, run={jam}");
    }
}
