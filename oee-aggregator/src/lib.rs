//! OEE aggregator (plan section 3): A × P × Q from `oee/line1/*` topics.
//!
//! Week 1: the formula stub. Week 5: the full runtime — MQTT subscription
//! (mqtt-min), event-time windows (minute + cumulative shift), publishing
//! `oee/line1/oee` for the dashboard, and the windows CSV (the raw material
//! of the measured-vs-truth experiment).

/// OEE = Availability × Performance × Quality (plan section 1.1).
pub fn oee(availability: f32, performance: f32, quality: f32) -> f32 {
    availability * performance * quality
}

/// The MQTT runtime, the fold core and the CSV log (week 5, D2).
pub mod aggregator;

/// The `oee/line1/oee` payload contract (D2/D3).
pub mod payload;

/// Per-window component math: A, P, Q over stored node streams.
pub mod windows;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_line() {
        assert!((oee(1.0, 1.0, 1.0) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_factor_zeroes_oee() {
        assert_eq!(oee(0.9, 0.0, 0.95), 0.0);
    }
}
