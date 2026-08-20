//! Калибровка тракта тока узла A: сырые отсчёты ADC → амперы.
//!
//! Контракт parity: одно и то же преобразование применяется в экспорте
//! обучения (захват с железа) и в инференсе прошивки — иначе модель видит
//! разные единицы. Номиналы стенда — ACS712-20A + делитель 2:1
//! (`kontext/20260820095259-equipment.md`, разд. 3 и 9).

/// Параметры тракта измерения тока узла A.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentCalibration {
    /// Полная шкала ADC в отсчётах (12 бит S3 → 4095).
    pub adc_full_scale: u16,
    /// Напряжение полной шкалы ADC, В (attenuation 11 дБ ≈ 3.1 В).
    pub adc_v_ref: f32,
    /// Коэффициент делителя на входе пина (2:1 → 2.0).
    pub divider: f32,
    /// Чувствительность датчика, В/А (ACS712-20A → 0.1).
    pub sensitivity_v_per_a: f32,
    /// Напряжение датчика при нулевом токе, В (ACS712 → VCC/2 = 2.5).
    pub sensor_zero_v: f32,
}

impl CurrentCalibration {
    /// Номиналы стенда: ACS712-20A (100 мВ/А, ноль 2.5 В) + делитель 2:1
    /// на входе ADC1 (3.3 В, 12 бит, attenuation 11 дБ).
    pub const fn acs712_20a_div2() -> Self {
        Self {
            adc_full_scale: 4095,
            adc_v_ref: 3.1,
            divider: 2.0,
            sensitivity_v_per_a: 0.1,
            sensor_zero_v: 2.5,
        }
    }

    /// Сырой отсчёт ADC → вольты на пине (линейное приближение шкалы).
    pub fn counts_to_pin_volts(&self, counts: u16) -> f32 {
        counts as f32 * self.adc_v_ref / self.adc_full_scale as f32
    }

    /// Сырой отсчёт ADC → амперы; единицы те же, что у `current_a`
    /// симулятора, — поэтому пайплайн фичей общий для обеих колей.
    pub fn counts_to_amps(&self, counts: u16) -> f32 {
        (self.counts_to_pin_volts(counts) * self.divider - self.sensor_zero_v)
            / self.sensitivity_v_per_a
    }

    /// Пересчитывает ноль датчика по среднему отсчёту ADC без нагрузки.
    ///
    /// Вызывать при старте прошивки: дрейф ACS712 и допуск резисторов
    /// делителя смещают «ноль»; без пересчёта холостой ток уезжает на
    /// десятки миллиампер.
    pub fn with_zero_counts(&self, zero_counts: u16) -> Self {
        let mut calib = *self;
        calib.sensor_zero_v = calib.counts_to_pin_volts(zero_counts) * calib.divider;
        calib
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Обратное преобразование: амперы → ожидаемый отсчёт ADC.
    fn amps_to_counts(calib: &CurrentCalibration, amps: f32) -> u16 {
        let pin_v = (amps * calib.sensitivity_v_per_a + calib.sensor_zero_v) / calib.divider;
        (pin_v / calib.adc_v_ref * calib.adc_full_scale as f32) as u16
    }

    #[test]
    fn zero_current_maps_to_zero_amps() {
        let calib = CurrentCalibration::acs712_20a_div2();
        // Номинальный ноль: датчик 2.5 В → пин 1.25 В.
        let counts = amps_to_counts(&calib, 0.0);
        assert!(calib.counts_to_amps(counts).abs() < 0.05, "counts={counts}");
    }

    #[test]
    fn drill_current_maps_linearly() {
        let calib = CurrentCalibration::acs712_20a_div2();
        // Режимы симулятора: idle 0.4 / run 2.0 / jam 3.2 / overload 4.5 А.
        for amps in [0.4_f32, 2.0, 3.2, 4.5] {
            let counts = amps_to_counts(&calib, amps);
            let measured = calib.counts_to_amps(counts);
            assert!(
                (measured - amps).abs() < 0.05,
                "amps={amps}, measured={measured}"
            );
        }
    }

    #[test]
    fn runtime_zero_correction_absorbs_sensor_drift() {
        let calib = CurrentCalibration::acs712_20a_div2();
        // Дрейф: ноль датчика съехал с 2.5 на 2.6 В — холостой ток «уезжает» ~1 А.
        let counts = |amps: f32| {
            (((2.6 + calib.sensitivity_v_per_a * amps) / calib.divider) / calib.adc_v_ref
                * calib.adc_full_scale as f32) as u16
        };
        assert!(calib.counts_to_amps(counts(0.0)) > 0.9, "дрейф не пойман");
        let corrected = calib.with_zero_counts(counts(0.0));
        assert!((corrected.counts_to_amps(counts(2.0)) - 2.0).abs() < 0.05);
        assert!(corrected.counts_to_amps(counts(0.0)).abs() < 0.05);
    }
}
