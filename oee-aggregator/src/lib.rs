//! OEE aggregator (plan section 3): A × P × Q from `oee/line1/*` topics.
//!
//! Week 1: skeleton stub. Formula and MQTT — week 5.

/// OEE = Availability × Performance × Quality (plan section 1.1).
pub fn oee(availability: f32, performance: f32, quality: f32) -> f32 {
    availability * performance * quality
}

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
