//! Node P firmware entry (Performance), the on-target half.
//!
//! The TCRT5000 barrier (`board::node_p::IR_OUT`, the CAM board): the pin
//! level is polled at 1 kHz (a contact bounce is ≥ ms-scale; polling makes
//! the 50 ms debounce deterministic — no interrupt-storm on a bouncing
//! contact), each counted part leaves UART0 as `p,<run_id>,<t_ms>,<count>`.
//!
//! Build layout: as in `firmware-a` — the esp-hal code is `app`, gated to
//! the Xtensa target; the host binary is an empty stub.

#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
mod app {
    use core::fmt::Write as _;

    use esp_hal::{
        delay::Delay,
        gpio::{Input, InputConfig, Pull},
        peripherals::Peripherals,
        uart::{Config as UartConfig, UartTx},
    };

    use firmware_p::{format_count, EdgeCounter, DEBOUNCE_MS};

    /// Run id of this firmware image.
    const RUN_ID: &str = "bench-p";

    /// The barrier poll period, ms.
    const POLL_MS: u32 = 1;

    #[esp_hal::main]
    fn main() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());

        let mut uart = UartTx::new(peripherals.UART0, UartConfig::default()).expect("uart0");
        let delay = Delay::new();

        // The TCRT5000 module OUT: push-pull (the comparator on the module);
        // a part in the gap pulls it high (the bench wiring convention — if
        // the bench shows inverted logic, flip it HERE, not in the counter).
        // GPIO5 = board::node_p::IR_OUT.
        let ir = Input::new(
            peripherals.GPIO5,
            InputConfig::default().with_pull(Pull::Down),
        );

        writeln!(uart, "p: boot, run_id={RUN_ID}").ok();

        let mut counter = EdgeCounter::new(DEBOUNCE_MS);
        let mut t_ms: u32 = 0;
        let mut line = [0u8; 48];

        loop {
            delay.delay_millis(POLL_MS);
            t_ms = t_ms.wrapping_add(POLL_MS);
            let level = ir.is_high();
            if let Some(count) = counter.observe(level, t_ms) {
                if let Some(n) = format_count(&mut line, RUN_ID, t_ms, count) {
                    uart.write(&line[..n]).ok();
                }
            }
        }
    }

    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let peripherals = unsafe { Peripherals::steal() };
        if let Ok(mut uart) = UartTx::new(peripherals.UART0, UartConfig::default()) {
            writeln!(uart, "p: panic {info}").ok();
        }
        loop {}
    }
}

// The host build: an empty binary — the node logic is the lib (tested).
#[cfg(not(target_arch = "xtensa"))]
fn main() {}
