//! The OEE QEMU firmware entry point (week 6, D2).
//!
//! Prints the model A predictions for the fixed windows over UART0, then
//! exits QEMU through the semihosting `exit` — the run is scriptable and
//! leaves a complete log (`scripts/qemu-parity.sh`).

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
