//! Current-path calibration for node A: raw ADC counts -> amps.
//!
//! Parity contract: the same conversion is applied in the training export
//! (hardware capture) and in firmware inference — otherwise the model sees
//! different units. Bench nominal values: ACS712-20A + 2:1 divider.

/// Parameters of the node A current-measurement path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CurrentCalibration {
    /// ADC full scale in counts (12-bit S3 -> 4095).
    pub adc_full_scale: u16,
    /// ADC full-scale voltage, V (attenuation 11 dB ≈ 3.1 V).
    pub adc_v_ref: f32,
    /// Divider ratio at the pin input (2:1 -> 2.0).
    pub divider: f32,
    /// Sensor sensitivity, V/A (ACS712-20A -> 0.1).
    pub sensitivity_v_per_a: f32,
    /// Sensor voltage at zero current, V (ACS712 -> VCC/2 = 2.5).
    pub sensor_zero_v: f32,
}

impl CurrentCalibration {
    /// Bench nominal values: ACS712-20A (100 mV/A, zero at 2.5 V) + a 2:1
    /// divider at the ADC1 input (3.3 V, 12-bit, attenuation 11 dB).
    pub const fn acs712_20a_div2() -> Self {
        Self {
            adc_full_scale: 4095,
            adc_v_ref: 3.1,
            divider: 2.0,
            sensitivity_v_per_a: 0.1,
            sensor_zero_v: 2.5,
        }
    }

    /// Raw ADC count -> volts at the pin (linear scale approximation).
    pub fn counts_to_pin_volts(&self, counts: u16) -> f32 {
        counts as f32 * self.adc_v_ref / self.adc_full_scale as f32
    }

    /// Raw ADC count -> amps; the units are the same as the simulator's
    /// `current_a`, so the feature pipeline is shared between both tracks.
    pub fn counts_to_amps(&self, counts: u16) -> f32 {
        (self.counts_to_pin_volts(counts) * self.divider - self.sensor_zero_v)
            / self.sensitivity_v_per_a
    }

    /// Recomputes the sensor zero from the averaged no-load ADC count.
    ///
    /// Call at firmware startup: ACS712 drift and divider resistor
    /// tolerances shift the "zero"; without recalibration the idle current
    /// drifts by tens of milliamps.
    pub fn with_zero_counts(&self, zero_counts: u16) -> Self {
        let mut calib = *self;
        calib.sensor_zero_v = calib.counts_to_pin_volts(zero_counts) * calib.divider;
        calib
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inverse conversion: amps -> expected ADC count.
    fn amps_to_counts(calib: &CurrentCalibration, amps: f32) -> u16 {
        let pin_v = (amps * calib.sensitivity_v_per_a + calib.sensor_zero_v) / calib.divider;
        (pin_v / calib.adc_v_ref * calib.adc_full_scale as f32) as u16
    }

    #[test]
    fn zero_current_maps_to_zero_amps() {
        let calib = CurrentCalibration::acs712_20a_div2();
        // Nominal zero: sensor 2.5 V -> pin 1.25 V.
        let counts = amps_to_counts(&calib, 0.0);
        assert!(calib.counts_to_amps(counts).abs() < 0.05, "counts={counts}");
    }

    #[test]
    fn drill_current_maps_linearly() {
        let calib = CurrentCalibration::acs712_20a_div2();
        // Simulator modes: idle 0.4 / run 2.0 / jam 3.2 / overload 4.5 A.
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
        // Drift: sensor zero moved from 2.5 to 2.6 V — the idle current drifts ~1 A.
        let counts = |amps: f32| {
            (((2.6 + calib.sensitivity_v_per_a * amps) / calib.divider) / calib.adc_v_ref
                * calib.adc_full_scale as f32) as u16
        };
        assert!(calib.counts_to_amps(counts(0.0)) > 0.9, "drift not caught");
        let corrected = calib.with_zero_counts(counts(0.0));
        assert!((corrected.counts_to_amps(counts(2.0)) - 2.0).abs() < 0.05);
        assert!(corrected.counts_to_amps(counts(0.0)).abs() < 0.05);
    }
}
