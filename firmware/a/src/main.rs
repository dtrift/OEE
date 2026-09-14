//! Node A firmware entry (Availability), the on-target half.
//!
//! ADC1 samples the ACS712 (through the 2:1 divider, `board::node_a`)
//! at ~1.6 kHz; a startup no-load average pins the sensor zero
//! (`CurrentCalibration::with_zero_counts`); 128-sample windows go through
//! the rust-born model and the anti-flap hysteresis; confirmed status
//! changes leave as `a,<run_id>,<t_ms>,<state>` lines over UART0
//! (115200-8N1, the USB-CDC bridge).
//!
//! Sampling cadence: a `Delay` loop (625 us) — the bring-up
//! simplification; a hardware timer chain is the first NOTES item if the
//! window-boundary jitter ever matters (the hysteresis absorbs it today).
//!
//! Build layout: the esp-hal code lives in `app`, gated to the Xtensa
//! target; on the host the binary is an empty stub (the node logic is the
//! `firmware_a` lib, host-tested) — that keeps `cargo test/clippy
//! --workspace` working without the esp toolchain.

#![cfg_attr(target_arch = "xtensa", no_std)]
#![cfg_attr(target_arch = "xtensa", no_main)]

#[cfg(target_arch = "xtensa")]
mod app {
    use core::fmt::Write as _;

    use esp_hal::{
        analog::adc::{Adc, Attenuation},
        delay::Delay,
        peripherals::Peripherals,
        uart::{Config as UartConfig, UartTx},
    };
    use features_cli::calibration::CurrentCalibration;

    use firmware_a::{
        advance_ms, classify, format_status, Hysteresis, WindowAccumulator, WindowOutcome,
        CONFIRM_AFTER, SAMPLE_US,
    };

    // The ESP-IDF app descriptor at the image head: the 2nd-stage
    // bootloader requires it, and espflash >= 4.6 refuses to flash an
    // image without it.
    esp_bootloader_esp_idf::esp_app_desc!();

    /// Run id of this firmware image (the offline-CSV family uses it verbatim).
    const RUN_ID: &str = "bench-a";

    /// Startup zero calibration: samples averaged at no load (0.5 s of rest).
    const ZERO_SAMPLES: u32 = 800;

    #[esp_hal::main]
    fn main() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());

        // Console + status lines: UART0 is wired to the USB-CDC bridge on
        // the DevKitC-1 (GPIO43 TX, fixed by the ROM bootloader contract).
        let mut uart = UartTx::new(peripherals.UART0, UartConfig::default()).expect("uart0");

        let delay = Delay::new();

        // ADC1 on the bench pin (ADC2 stays free for WiFi — the board
        // contract). GPIO4 = board::node_a::ADC_CURRENT.
        let mut adc_config = esp_hal::analog::adc::AdcConfig::new();
        let mut adc_pin = adc_config.enable_pin(peripherals.GPIO4, Attenuation::_11dB);
        let mut adc = Adc::new(peripherals.ADC1, adc_config);

        writeln!(uart, "a: boot, run_id={RUN_ID}").ok();

        // Startup zero: average the no-load signal, absorb the ACS712 drift
        // (the S1 check: at rest the reading is ~0 A within tolerance).
        let mut zero_sum: u64 = 0;
        for _ in 0..ZERO_SAMPLES {
            zero_sum += adc.read_blocking(&mut adc_pin) as u64;
            delay.delay_micros(SAMPLE_US);
        }
        let zero_counts = (zero_sum / ZERO_SAMPLES as u64) as u16;
        let calibration = CurrentCalibration::acs712_20a_div2().with_zero_counts(zero_counts);
        writeln!(uart, "a: zero={zero_counts}").ok();

        let mut window_acc = WindowAccumulator::new();
        let mut hysteresis = Hysteresis::new(CONFIRM_AFTER);
        let mut t_us: u64 = 0;
        let mut line = [0u8; 96];

        loop {
            delay.delay_micros(SAMPLE_US);
            let t_ms = advance_ms(&mut t_us, SAMPLE_US);
            // `read_blocking` retries internally; a hard failure is a
            // reboot-grade event and simply not expected at the bench.
            let sample = Some(calibration.counts_to_amps(adc.read_blocking(&mut adc_pin)));
            match window_acc.push(sample) {
                WindowOutcome::Complete(window) => {
                    if let Some(state) = hysteresis.observe(classify(&window).1) {
                        if let Some(n) = format_status(&mut line, RUN_ID, t_ms, state) {
                            uart.write(&line[..n]).ok();
                        }
                    }
                }
                WindowOutcome::Dirty => { /* the window is dropped, the stream lives */ }
                WindowOutcome::Filling => {}
            }
        }
    }

    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        // Best effort: a panic line on the console, then halt (the board
        // gets power-cycled at the bench; no reset-loop hiding the message).
        let peripherals = unsafe { Peripherals::steal() };
        if let Ok(mut uart) = UartTx::new(peripherals.UART0, UartConfig::default()) {
            writeln!(uart, "a: panic {info}").ok();
        }
        loop {}
    }
}

// The host build: an empty binary — the node logic is the lib (tested).
#[cfg(not(target_arch = "xtensa"))]
fn main() {}
