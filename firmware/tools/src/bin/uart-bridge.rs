//! The phase-2 UART→MQTT bridge (the hardware track, decompose S7):
//!
//!     cat /dev/ttyUSB0 | cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
//!
//! Reads the boards' status lines from stdin (a terminal pipes the USB-CDC
//! port in), publishes each on `oee/line1/*`, and emits the `{node}/end`
//! markers when the stream closes — the week-5 aggregator and dashboard
//! then work against the physical bench unmodified.

use std::io::BufReader;

fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:1883".to_string());
    let input = BufReader::new(std::io::stdin().lock());
    let mut out = std::io::stdout().lock();
    match firmware_tools::bridge::run_bridge(input, &addr, &mut out) {
        Ok(count) => eprintln!("uart-bridge: done, {count} messages"),
        Err(error) => {
            eprintln!("uart-bridge: {error}");
            std::process::exit(1);
        }
    }
}
