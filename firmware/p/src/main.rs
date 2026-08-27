//! Node P firmware (Performance): TCRT5000 IR barrier (OUT -> GPIO5,
//! [`board`]) -> edge detector with ~50 ms debounce -> part counting.
//!
//! Detector logic is a pure core (host-tested in `nodes`); the firmware is
//! only GPIO/interrupt plumbing. The second "belt end" barrier is the same
//! driver on another pin (assigned at bench assembly).

fn main() {
    // TODO(shakedown): #![no_std] + esp-hal; steps — ../README.md.
}
