//! Контракт источника данных узла (разд. 2 плана: «что заменило железо»).
//!
//! Один trait для двух колей разработки: `SimSource` на хосте (неделя 4,
//! читает поток симулятора) и сенсорные источники прошивки (`AdcSource` —
//! ACS712 через ADC1, `I2sSource` — INMP441, `GpioEdgeSource` — TCRT5000).
//!
//! Контракт сознательно no_std-совместим: без аллокаций, `String` и
//! `anyhow` — чтобы при извлечении ядра узла в прошивку (недели 4–5)
//! trait переехал без изменений.

/// Ошибка источника: поток исчерпан или отказ сенсорного тракта.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// Источник закончился (симулятор — по длительности сценария).
    Exhausted,
    /// Отказ сенсорного тракта; строка — место отказа, без аллокации.
    Sensor(&'static str),
}

/// Поток отсчётов узла: симулятор (хост) или сенсор (прошивка).
///
/// Узел не знает, откуда данные: пайплайн «фичи → predict → публикация»
/// один и тот же для SimSource (хост) и железных источников (прошивка).
pub trait SensorSource {
    /// Тип отсчёта: ток в амперах, сырой отсчёт ADC, фронт IR-барьера...
    type Sample;

    /// Следующий отсчёт потока.
    fn next_sample(&mut self) -> Result<Self::Sample, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Простейший источник для проверки формы контракта.
    struct Countdown(u32);

    impl SensorSource for Countdown {
        type Sample = u32;

        fn next_sample(&mut self) -> Result<u32, SourceError> {
            if self.0 == 0 {
                Err(SourceError::Exhausted)
            } else {
                self.0 -= 1;
                Ok(self.0)
            }
        }
    }

    #[test]
    fn countdown_yields_samples_then_exhausts() {
        let mut src = Countdown(2);
        assert_eq!(src.next_sample(), Ok(1));
        assert_eq!(src.next_sample(), Ok(0));
        assert_eq!(src.next_sample(), Err(SourceError::Exhausted));
    }
}
