//! Node Q firmware entry (Quality), the on-target half.
//!
//! The tap cycle: the servo strikes (a 50 Hz PWM pulse train via LEDC,
//! `board::node_q::SERVO_PWM`), a settle pause, then one 64 ms window is
//! classified by the rust-born model and the verdict leaves UART0 as
//! `q,<run_id>,<t_ms>,<verdict>`.
//!
//! Bring-up staging (decompose S4/S5): the I2S/INMP441 input is the next
//! step — until it lands, the window source is a deterministic synthetic
//! tap (quiet decay), so the servo→window→verdict→UART loop is verifiable
//! on the bench end-to-end; the `i2s_slots_to_f32` conversion in the lib
//! is already pinned by host tests.
//!
//! Servo power: the separate 5 V supply + 470 µF (the firmware/README
//! condition — a rail sag reboots the board mid-tap).
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
        ledc::{
            channel::{Channel, ChannelIFace},
            timer::{
                config::{Config as TimerConfig, Duty},
                LSClockSource, Number as TimerNumber, TimerIFace,
            },
            Ledc, LowSpeed,
        },
        peripherals::Peripherals,
        time::Rate,
        uart::{Config as UartConfig, UartTx},
    };

    use firmware_q::{classify, format_verdict, WINDOW};

    /// Run id of this firmware image.
    const RUN_ID: &str = "bench-q";

    /// SG90-style servo: 50 Hz frame; strike ~1.2 ms / rest ~1.6 ms of duty.
    const SERVO_HZ: u32 = 50;
    const STRIKE_DUTY_PCT: u8 = 6;
    const REST_DUTY_PCT: u8 = 8;

    /// Pause between the strike and the window start, ms (the hammer
    /// leaves, the ring begins).
    const SETTLE_MS: u32 = 30;

    /// Pause between taps, ms (the bench feed rate).
    const TAP_PERIOD_MS: u32 = 400;

    #[esp_hal::main]
    fn main() -> ! {
        let peripherals = esp_hal::init(esp_hal::Config::default());

        let mut uart = UartTx::new(peripherals.UART0, UartConfig::default()).expect("uart0");
        let delay = Delay::new();

        // The servo: LEDC low-speed timer at 50 Hz (14-bit duty) + a
        // channel on the bench pin. Duty in percent via ChannelIFace: at a
        // 20 ms frame, 1% = 0.2 ms — the SG90 strike/rest split (≈1.2/1.6
        // ms) is within that resolution (the fine trim is a bench-assembly
        // knob, the horn geometry, not software).
        // GPIO11 = board::node_q::SERVO_PWM.
        let ledc = Ledc::new(peripherals.LEDC);
        let mut timer = ledc.timer::<LowSpeed>(TimerNumber::Timer0);
        timer
            .configure(TimerConfig {
                duty: Duty::Duty14Bit,
                clock_source: LSClockSource::APBClk,
                frequency: Rate::from_hz(SERVO_HZ),
            })
            .expect("ledc timer");
        let mut servo =
            ledc.channel::<LowSpeed>(esp_hal::ledc::channel::Number::Channel0, peripherals.GPIO11);

        writeln!(uart, "q: boot, run_id={RUN_ID}").ok();

        let mut t_ms: u32 = 0;
        let mut line = [0u8; 64];

        loop {
            // Strike and release (the duty step swings the horn into the
            // part).
            set_duty(&mut servo, STRIKE_DUTY_PCT);
            delay.delay_millis(SETTLE_MS);
            set_duty(&mut servo, REST_DUTY_PCT);
            t_ms = t_ms.wrapping_add(SETTLE_MS);

            // The window (S4 TODO: I2S INMP441; a deterministic synthetic
            // tap until then — quiet decay, the good-part family).
            let mut window = [0.0f32; WINDOW];
            for (i, slot) in window.iter_mut().enumerate() {
                let t = i as f32 / WINDOW as f32;
                *slot = libm::sinf(t * 120.0 * core::f32::consts::PI) * (1.0 - t) * 0.05;
            }

            let verdict = classify(&window).1;
            if let Some(n) = format_verdict(&mut line, RUN_ID, t_ms, verdict) {
                uart.write(&line[..n]).ok();
            }

            // The rest of the tap period.
            delay.delay_millis(TAP_PERIOD_MS - SETTLE_MS);
            t_ms = t_ms.wrapping_add(TAP_PERIOD_MS - SETTLE_MS);
        }
    }

    /// Sets the servo duty in percent (the esp-hal channel trait; the
    /// signal runs as soon as the channel is configured).
    fn set_duty(servo: &mut Channel<'_, LowSpeed>, pct: u8) {
        let _ = servo.set_duty(pct);
    }

    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        let peripherals = unsafe { Peripherals::steal() };
        if let Ok(mut uart) = UartTx::new(peripherals.UART0, UartConfig::default()) {
            writeln!(uart, "q: panic {info}").ok();
        }
        loop {}
    }
}

// The host build: an empty binary — the node logic is the lib (tested).
#[cfg(not(target_arch = "xtensa"))]
fn main() {}
