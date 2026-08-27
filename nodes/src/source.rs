//! Node data-source contract (plan section 2: "what replaces the hardware").
//!
//! One trait for both development tracks: `SimSource` on the host (week 4,
//! reads the simulator stream) and firmware sensor sources (`AdcSource` —
//! ACS712 via ADC1, `I2sSource` — INMP441, `GpioEdgeSource` — TCRT5000).
//!
//! The contract is deliberately no_std-compatible: no allocations, `String`,
//! or `anyhow` — so that when the node core moves into firmware (weeks 4-5)
//! the trait carries over unchanged.

/// Source error: the stream is exhausted or the sensor path failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceError {
    /// The source has ended (for the simulator, at scenario duration).
    Exhausted,
    /// Sensor-path failure; the string is the failure location, allocation-free.
    Sensor(&'static str),
}

/// Node sample stream: simulator (host) or sensor (firmware).
///
/// The node does not know where the data comes from: the "features ->
/// predict -> publish" pipeline is the same for SimSource (host) and
/// hardware sources (firmware).
pub trait SensorSource {
    /// Sample type: current in amps, raw ADC count, IR-barrier edge...
    type Sample;

    /// The next sample of the stream.
    fn next_sample(&mut self) -> Result<Self::Sample, SourceError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simplest source to check the contract shape.
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
