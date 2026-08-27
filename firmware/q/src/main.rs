//! Node Q firmware (Quality): tap test -> pass/fail verdict.
//!
//! Orchestration: 50 Hz servo PWM ([`board`], separate 5 V supply) ->
//! stick strike on the part -> settle pause -> I2S window
//! (`features-cli::window_spec`, 16 kHz) -> `predict()` -> verdict.
//!
//! The node must survive the supply sag from the servo inrush current:
//! a capacitor at the servo pins; on a brownout reboot, continue the cycle
//! (idempotency: an unfinished tap does not count as a verdict).

fn main() {
    // TODO(shakedown): #![no_std] + esp-hal; steps — ../README.md.
}
