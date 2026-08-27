//! Node A firmware (Availability): ACS712-20A -> ADC1 (GPIO4, [`board`]) ->
//! `features-cli::calibration` calibration (zero via startup no-load
//! averaging) -> 128-sample window @ 1.6 kHz (`features-cli::window_spec`) ->
//! `predict()` -> status.
//!
//! Skeleton: builds on the host without the esp toolchain. First-version
//! plan: statuses over USB-CDC (the same CSV schema as in
//! `features-cli::capture`), MQTT via esp-wifi — after the wired end-to-end.

fn main() {
    // TODO(shakedown): #![no_std] + esp-hal; steps — ../README.md.
}
