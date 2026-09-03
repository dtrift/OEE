//! MQTT publishing sink (week 4, D2): statuses/verdicts to
//! `oee/line1/{a,q}/...` over the minimal own client.
//!
//! Robustness contract (the D5 error-isolation rule): publishing must never
//! kill the node. On connect/publish failure the sink returns `false`, the
//! node keeps writing the offline CSV, and the next status retries the
//! connection after a capped linear backoff. Payloads are hand-formatted
//! JSON (no serde_json dependency): tiny fixed shapes, escaped-free fields
//! (run ids are validated by the CLI).

use std::time::Duration;

use mqtt_min::Client;

use crate::status::{StatusRow, StatusSink};

/// MQTT topic layout of the OEE line (the aggregator contract, week 5).
pub mod topics {
    /// Node A status changes: JSON `{state,t_ms,run_id}`.
    pub const A_STATUS: &str = "oee/line1/a/status";
    /// Node A metadata (published once at startup): model + WindowSpec.
    pub const A_META: &str = "oee/line1/a/meta";
    /// Node Q verdicts: JSON `{verdict,t_ms,run_id}`.
    pub const Q_VERDICT: &str = "oee/line1/q/verdict";
    /// Node Q metadata (published once at startup).
    pub const Q_META: &str = "oee/line1/q/meta";
}

/// An MQTT sink with lazy connect and capped-backoff reconnect.
pub struct MqttSink {
    addr: String,
    client_id: String,
    client: Option<Client>,
    /// Failed attempts since the last success (backoff = attempts capped).
    attempts: u32,
    publishes: usize,
    failures: usize,
}

impl MqttSink {
    /// `addr` is `host:port`; the client connects lazily on first publish.
    pub fn new(addr: &str, client_id: &str) -> Self {
        Self {
            addr: addr.to_string(),
            client_id: client_id.to_string(),
            client: None,
            attempts: 0,
            publishes: 0,
            failures: 0,
        }
    }

    /// Successful publishes so far.
    pub fn publishes(&self) -> usize {
        self.publishes
    }

    /// Failed publish/connect attempts so far (the error-isolation counter).
    pub fn failures(&self) -> usize {
        self.failures
    }

    /// Backoff before the next reconnect attempt: linear, capped at 3.
    fn backoff(&self) -> Duration {
        Duration::from_millis(50 * self.attempts.min(3) as u64)
    }

    /// Publishes once, reconnecting if needed; `Ok(false)` = broker still
    /// unreachable (the node continues offline).
    pub fn publish(&mut self, topic: &str, payload: &str) -> bool {
        if self.client.is_none() {
            std::thread::sleep(self.backoff());
            match Client::connect(&self.addr, &self.client_id, 60) {
                Ok(client) => {
                    self.client = Some(client);
                    self.attempts = 0;
                }
                Err(_) => {
                    self.attempts += 1;
                    self.failures += 1;
                    return false;
                }
            }
        }
        let published = self
            .client
            .as_mut()
            .map(|client| client.publish(topic, payload))
            .map(|result| result.is_ok());
        match published {
            Some(true) => {
                self.publishes += 1;
                true
            }
            _ => {
                // Broken pipe etc: drop the connection, count, continue.
                self.client = None;
                self.attempts += 1;
                self.failures += 1;
                false
            }
        }
    }

    /// Publishes the node A meta line (model version + WindowSpec).
    pub fn publish_a_meta(&mut self, model: &str, samples: usize, rate_hz: u32) -> bool {
        let payload = format!(
            r#"{{"model":"{model}","window_samples":{samples},"sample_rate_hz":{rate_hz}}}"#
        );
        self.publish(topics::A_META, &payload)
    }

    /// Publishes the node Q meta line.
    pub fn publish_q_meta(&mut self, model: &str, samples: usize, rate_hz: u32) -> bool {
        let payload = format!(
            r#"{{"model":"{model}","window_samples":{samples},"sample_rate_hz":{rate_hz}}}"#
        );
        self.publish(topics::Q_META, &payload)
    }
}

impl StatusSink for MqttSink {
    fn on_status(&mut self, row: &StatusRow) {
        // The topic/payload shape is per-node; the row carries the node.
        let (topic, key) = if row.node == "a" {
            (topics::A_STATUS, "state")
        } else {
            (topics::Q_VERDICT, "verdict")
        };
        let payload = format!(
            r#"{{"{key}":"{}","t_ms":{},"run_id":"{}"}}"#,
            row.state, row.t_ms, row.run_id
        );
        self.publish(topic, &payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::VecSink;
    use mqtt_min::testing::LoopbackBroker;

    fn row(node: &'static str, state: &str, t_ms: u32) -> StatusRow {
        StatusRow {
            node,
            run_id: "run1".into(),
            t_ms,
            state: state.into(),
        }
    }

    #[test]
    fn statuses_reach_the_broker_with_the_pinned_shapes() {
        let broker = LoopbackBroker::spawn();
        let mut sink = MqttSink::new(&broker.addr, "node-a");
        assert!(sink.publish_a_meta("model_a.tflite", 128, 1600));
        sink.on_status(&row("a", "run", 1250));
        sink.on_status(&row("q", "cracked", 400));

        let first = broker
            .publishes
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(first.topic, "oee/line1/a/meta");
        assert_eq!(
            first.payload,
            r#"{"model":"model_a.tflite","window_samples":128,"sample_rate_hz":1600}"#
        );
        let second = broker
            .publishes
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(second.topic, "oee/line1/a/status");
        assert_eq!(
            second.payload,
            r#"{"state":"run","t_ms":1250,"run_id":"run1"}"#
        );
        let third = broker
            .publishes
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(third.topic, "oee/line1/q/verdict");
        assert_eq!(
            third.payload,
            r#"{"verdict":"cracked","t_ms":400,"run_id":"run1"}"#
        );
        assert_eq!(sink.publishes(), 3);
    }

    #[test]
    fn dead_broker_degrades_to_offline_without_panicking() {
        // A claimed-then-freed port: nothing listens there.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        drop(listener);

        let mut sink = MqttSink::new(&addr, "node-a");
        sink.on_status(&row("a", "run", 1250));
        sink.on_status(&row("a", "jam", 2500));
        assert_eq!(sink.publishes(), 0, "nothing can be published");
        assert_eq!(sink.failures(), 2);
        // The statuses still flow to any parallel offline sink.
        let mut offline = VecSink::default();
        offline.on_status(&row("a", "run", 1250));
        assert_eq!(offline.0.len(), 1);
    }

    #[test]
    fn backoff_is_capped() {
        let mut sink = MqttSink::new("127.0.0.1:1", "x");
        for _ in 0..10 {
            sink.publish("t/opic", "x");
        }
        // Linear growth capped at 3 attempts -> max 150 ms per retry.
        assert_eq!(sink.backoff(), Duration::from_millis(150));
    }
}
