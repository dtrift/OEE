//! The reshape-trick footprint variant (week 6, D3).
//!
//! Same model, same windows, same output as `oee-qemu` — but built with
//! `MICROFLOW_CONV2D_ONLY=1`, which makes the fork's `#[model]` take every
//! 1-D convolution through the generic `conv_2d` kernel (the pre-Conv1D
//! state: the model-as-CONV_2D trick). The binary exists to be *measured*
//! (`scripts/footprint.sh`); the default build does not compile it
//! (`required-features = ["footprint-conv2d"]`), so a normal `cargo build`
//! never carries the trick by accident.
//!
//! Build trap: the env var is read inside the proc macro, and cargo does
//! NOT fingerprint proc-macro env reads — this bin must be built with its
//! own `CARGO_TARGET_DIR` (scripts/footprint.sh does exactly that).

#![no_std]
#![no_main]

use cortex_m::asm::nop;
use cortex_m_rt::entry;
use cortex_m_semihosting::debug::{exit, EXIT_SUCCESS};
use panic_halt as _;

#[entry]
fn main() -> ! {
    oee_qemu::run();
    exit(EXIT_SUCCESS);
    loop {
        nop()
    }
}
