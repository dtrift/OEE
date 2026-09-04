//! A tiny MQTT broker for the offline bench (week 5, D6):
//!
//!     cargo run -p mqtt-min --bin broker [--port 1883]
//!
//! The same core as `mqtt_min::testing::LoopbackBroker`, bound to a fixed
//! port: accepts any number of clients, CONNACK/SUBACK/PINGRESP, QoS-0
//! PUBLISH dispatch to matching subscribers (`#`/`+` wildcards). This is the
//! offline stand-in for `mosquitto` in the one-command bench
//! (`scripts/bench.sh`) — nodes, the aggregator and the dashboard connect to
//! it exactly as they would to a real broker. No persistence, no QoS 1+,
//! no auth: a bench broker, not a production one. Ctrl-C stops it.

use mqtt_min::testing::LoopbackBroker;

fn main() {
    let port = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse::<u16>().ok())
        .unwrap_or(1883);
    let broker = match LoopbackBroker::bind(&format!("127.0.0.1:{port}")) {
        Ok(broker) => broker,
        Err(error) => {
            eprintln!("mqtt-min broker: cannot bind 127.0.0.1:{port}: {error}");
            std::process::exit(1);
        }
    };
    println!("mqtt-min broker listening on {}", broker.addr);
    // The accept loop runs on its own (detached) threads; keep the main
    // thread alive until Ctrl-C.
    loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
    }
}
