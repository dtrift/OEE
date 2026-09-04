//! The MQTT thread (D3): connect, subscribe `oee/line1/#`, stream messages
//! into the UI thread over an mpsc channel, reconnect with backoff when the
//! broker drops. The week-4 nodes only publish — this is the first
//! subscribing consumer, on the same `mqtt-min` wire path as the aggregator.

use std::sync::mpsc::Sender;
use std::time::Duration;

use mqtt_min::Client;

/// What the MQTT thread tells the UI thread.
pub enum MqttEvent {
    /// A received PUBLISH (topic + payload).
    Message { topic: String, payload: String },
    /// Connected (initially and after each reconnect).
    Connected,
    /// The connection dropped; a reconnect is scheduled.
    Disconnected,
    /// One idle read cycle passed — also the liveness check: sending it
    /// notices a dropped UI (the thread then exits instead of idling
    /// forever on a silent broker).
    Tick,
}

/// How long one idle read waits before checking the reconnect clock.
const READ_TIMEOUT: Duration = Duration::from_millis(500);
/// Reconnect backoff: fixed 1 s (the broker restarts fast on the bench;
/// keeping it simple beats exponential curves here).
const RECONNECT_AFTER: Duration = Duration::from_secs(1);

/// Runs the MQTT loop until the receiver end of `events` is dropped (the UI
/// thread owns it; dropping = exit). Never panics: every error degrades to
/// a reconnect cycle.
pub fn run_loop(broker_addr: String, filter: String, events: Sender<MqttEvent>) {
    loop {
        let mut client = match Client::connect(&broker_addr, "oee-dashboard", 60) {
            Ok(client) => client,
            Err(_) => {
                if events.send(MqttEvent::Disconnected).is_err() {
                    return; // UI gone: exit
                }
                std::thread::sleep(RECONNECT_AFTER);
                continue;
            }
        };
        if client.subscribe(&filter).is_err() {
            if events.send(MqttEvent::Disconnected).is_err() {
                return;
            }
            std::thread::sleep(RECONNECT_AFTER);
            continue;
        }
        if events.send(MqttEvent::Connected).is_err() {
            return;
        }
        loop {
            match client.next_message(READ_TIMEOUT) {
                Ok(Some(message)) => {
                    if events
                        .send(MqttEvent::Message {
                            topic: message.topic,
                            payload: message.payload,
                        })
                        .is_err()
                    {
                        return; // UI gone: exit
                    }
                }
                Ok(None) => {
                    if events.send(MqttEvent::Tick).is_err() {
                        return; // UI gone: exit
                    }
                }
                Err(_) => break, // broker dropped: reconnect
            }
        }
        if events.send(MqttEvent::Disconnected).is_err() {
            return;
        }
        std::thread::sleep(RECONNECT_AFTER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Instant;

    /// The loopback end-to-end: a publisher sends an OEE payload, the
    /// thread delivers it as an event, the dashboard state consumes it.
    #[test]
    fn messages_flow_from_the_broker_into_events() {
        let broker = mqtt_min::testing::LoopbackBroker::spawn();
        let (tx, rx) = mpsc::channel();
        let addr = broker.addr.clone();
        let thread = std::thread::spawn(move || run_loop(addr, "oee/line1/#".into(), tx));

        // Wait for the subscribe (Connected is sent after the SUBACK).
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).expect("an event") {
                MqttEvent::Connected => break,
                MqttEvent::Message { .. } | MqttEvent::Tick => {}
                MqttEvent::Disconnected => panic!("broker is up; must not disconnect"),
            }
        }

        let mut publisher = Client::connect(&broker.addr, "publisher", 60).unwrap();
        publisher
            .publish(
                "oee/line1/oee",
                r#"{"scope":"shift","run_id":"t-1","t_from_ms":0,"t_to_ms":60000,"planned_ms":60000,"run_ms":60000,"parts":150,"good":150,"total":150,"a":1.000,"p":1.000,"q":1.000,"oee":1.000}"#,
            )
            .unwrap();

        let mut state = crate::state::DashboardState::new(&broker.addr);
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_secs(5)).expect("an event") {
                MqttEvent::Message { topic, payload } => {
                    state.on_message(&topic, &payload, Instant::now());
                    if topic == "oee/line1/oee" {
                        break;
                    }
                }
                MqttEvent::Tick | MqttEvent::Connected => {}
                MqttEvent::Disconnected => panic!("broker is up"),
            }
        }
        assert!(
            (state.shift.oee - 1.0).abs() < 1e-3,
            "payload reached the state"
        );

        // Dropping the receiver ends the thread (join without hanging).
        drop(rx);
        thread.join().expect("clean exit");
    }
}
