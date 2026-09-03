//! Digital-twin nodes (plan section 3): A (current), P (counting), Q (acoustics).
//!
//! Week 1: skeleton stubs. Inference via `#[model]` — weeks 3-4,
//! MQTT publishing — weeks 4-5.
//!
//! Hardware track (parallel): [`source::SensorSource`] is the data-source
//! contract, laid down before week 4 so the features don't fuse with the
//! simulator; sensor implementations arrive with the firmware (`firmware/`).

/// Node A: machine current -> features -> 1D-CNN -> status (idle/run/jam/overload).
pub mod a;

/// MQTT publishing sink + topics (week 4).
pub mod mqtt_sink;

/// Node P: IR barrier -> edge detector -> part counting.
pub mod p;

/// Node Q: tap test -> audio synthesis -> 1D-CNN -> pass/fail.
pub mod q;

/// Node data-source contract: SimSource (host) / sensors (firmware).
pub mod source;

/// CSV-backed sources over the simulator exports (week 4).
pub mod sim_source;

/// Window assembly, hysteresis, status sinks (week 4).
pub mod status;

#[cfg(test)]
mod tests {
    #[test]
    fn skeleton_imports() {
        // Check that the node modules compile.
        let _ = super::a::describe();
        let _ = super::p::describe();
        let _ = super::q::describe();
    }
}
