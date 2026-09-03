//! In-process test infrastructure: a loopback MQTT broker.
//!
//! Speaks just enough MQTT 3.1.1 to exercise [`crate::Client`]
//! end-to-end — the offline stand-in for `mosquitto` (D0's
//! `mosquitto_sub` check stays a user action; see the week-4 gate).
//! Besides CONNACK/PINGRESP and PUBLISH capture (week 4), since week 5 it
//! grants SUBSCRIBEs and dispatches incoming PUBLISHes to every matching
//! subscriber (`#` and `+` wildcards), so the aggregator and the dashboard
//! are tested on the same wire path as production. The `nodes` integration
//! tests reuse this broker through the dev-dependency, and the `broker`
//! binary runs the same core as the offline bench broker.
//!
//! Concurrency contract (the week-4 retro lesson: a broker is concurrent by
//! definition): one reader + one writer thread per connection. A publisher's
//! packets are dispatched in arrival order; per-subscriber delivery order is
//! therefore preserved per publisher (interleaving across publishers is
//! arbitrary — the aggregator's event-time watermark absorbs that).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

/// One captured publish: topic + payload.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedPublish {
    pub topic: String,
    pub payload: String,
}

/// A registered subscription: a filter plus the connection's outbound queue.
struct Subscription {
    conn_id: u64,
    filter: String,
    outbox: Sender<Vec<u8>>,
}

/// Broker-wide state shared by all connection threads.
struct Shared {
    next_conn_id: u64,
    subscriptions: Vec<Subscription>,
}

/// A spawned loopback broker; `publishes` yields captured PUBLISHes in
/// arrival order. Server threads are detached (process lifetime is the
/// test lifetime; ports are ephemeral).
pub struct LoopbackBroker {
    pub addr: String,
    pub publishes: Receiver<CapturedPublish>,
}

impl LoopbackBroker {
    /// Starts the broker on an ephemeral localhost port.
    pub fn spawn() -> Self {
        Self::bind("127.0.0.1:0").expect("bind loopback")
    }

    /// Binds the broker to a specific address (the `broker` binary).
    pub fn bind(addr: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr)?;
        let addr = listener.local_addr()?.to_string();
        let (tx, publishes) = channel();
        let shared = Arc::new(Mutex::new(Shared {
            next_conn_id: 0,
            subscriptions: Vec::new(),
        }));
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let tx = tx.clone();
                let shared = Arc::clone(&shared);
                // One reader + one writer thread per connection: nodes hold
                // their connections open concurrently, and subscribers get
                // PUBLISHes pushed from other connections' reader threads.
                thread::spawn(move || serve_connection(&mut stream, &tx, &shared));
            }
        });
        Ok(Self { addr, publishes })
    }
}

/// Serves one connection until EOF: CONNACK on CONNECT, SUBACK + dispatch
/// registration on SUBSCRIBE, PINGRESP on PINGREQ, capture + fan-out on
/// PUBLISH. All writes go through the connection's outbox (single writer
/// thread — readers never race on the socket).
fn serve_connection(
    stream: &mut TcpStream,
    capture: &Sender<CapturedPublish>,
    shared: &Mutex<Shared>,
) {
    let conn_id = {
        let mut guard = shared.lock().expect("broker state lock");
        guard.next_conn_id += 1;
        guard.next_conn_id
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    let Ok(mut writer_stream) = stream.try_clone() else {
        return;
    };
    let (outbox, outbox_rx) = channel::<Vec<u8>>();
    // The writer thread owns the cloned stream; it ends when the outbox
    // closes (reader exit + subscription cleanup) or a write fails.
    thread::spawn(move || {
        for packet in outbox_rx {
            if writer_stream.write_all(&packet).is_err() {
                break;
            }
            let _ = writer_stream.flush();
        }
    });
    let _ = serve_reads(stream, capture, shared, conn_id, &outbox);
    // On exit, drop this connection's subscriptions so the writer thread
    // finishes and long-running brokers do not accumulate dead entries.
    if let Ok(mut guard) = shared.lock() {
        guard.subscriptions.retain(|sub| sub.conn_id != conn_id);
    }
}

/// The reader loop of one connection (see [`serve_connection`]); ends with
/// `Err` on EOF or a transport error — the caller then cleans up.
fn serve_reads(
    stream: &mut TcpStream,
    capture: &Sender<CapturedPublish>,
    shared: &Mutex<Shared>,
    conn_id: u64,
    outbox: &Sender<Vec<u8>>,
) -> std::io::Result<()> {
    loop {
        let first = read_u8(stream)?;
        let len = read_remaining_length(stream)?;
        let mut body = vec![0u8; len];
        stream.read_exact(&mut body)?;
        match first {
            0x10 => {
                // CONNECT -> CONNACK(0).
                let _ = outbox.send(vec![0x20, 0x02, 0x00, 0x00]);
            }
            0x82 => {
                // SUBSCRIBE -> SUBACK(0x00 per filter), then register the
                // filters. The SUBACK is queued before registration, so no
                // dispatch can overtake it in the outbox.
                let Some(packet_id) = packet_id(&body) else {
                    continue;
                };
                let filters = filters(&body);
                let mut suback = vec![0x90, (2 + filters.len()) as u8];
                suback.extend_from_slice(&packet_id.to_be_bytes());
                suback.extend(std::iter::repeat_n(0x00, filters.len()));
                let _ = outbox.send(suback);
                if let Ok(mut guard) = shared.lock() {
                    for filter in filters {
                        guard.subscriptions.push(Subscription {
                            conn_id,
                            filter,
                            outbox: outbox.clone(),
                        });
                    }
                }
            }
            0x30 => {
                // PUBLISH QoS 0: capture + dispatch to matching subscribers.
                if body.len() < 2 {
                    continue;
                }
                let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
                if body.len() < 2 + topic_len {
                    continue;
                }
                let topic = String::from_utf8_lossy(&body[2..2 + topic_len]).into_owned();
                let payload = String::from_utf8_lossy(&body[2 + topic_len..]).into_owned();
                let _ = capture.send(CapturedPublish {
                    topic: topic.clone(),
                    payload: payload.clone(),
                });
                if let Ok(guard) = shared.lock() {
                    for sub in &guard.subscriptions {
                        if topic_matches(&sub.filter, &topic) {
                            // A dead subscriber (dropped receiver end) is
                            // skipped, not an error.
                            let _ = sub.outbox.send(crate::encode_publish(&topic, &payload));
                        }
                    }
                }
            }
            0xC0 => {
                // PINGREQ -> PINGRESP.
                let _ = outbox.send(vec![0xD0, 0x00]);
            }
            _ => {}
        }
    }
}

/// MQTT topic-filter matching: `#` matches any number of trailing levels
/// (including the parent — `sport/#` matches `sport` per the spec), `+`
/// matches exactly one level.
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    let filter: Vec<&str> = filter.split('/').collect();
    let topic: Vec<&str> = topic.split('/').collect();
    let mut i = 0;
    loop {
        if i == filter.len() {
            return i == topic.len();
        }
        if filter[i] == "#" {
            return true;
        }
        if i == topic.len() {
            return false;
        }
        if filter[i] != "+" && filter[i] != topic[i] {
            return false;
        }
        i += 1;
    }
}

/// The packet id (bytes 0..2) of a SUBSCRIBE body, if present.
fn packet_id(body: &[u8]) -> Option<u16> {
    if body.len() < 2 {
        return None;
    }
    Some(u16::from_be_bytes([body[0], body[1]]))
}

/// The topic filters of a SUBSCRIBE body: (length-prefixed filter, qos)
/// pairs after the packet id. Malformed pairs are skipped.
fn filters(body: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = 2;
    while at + 2 <= body.len() {
        let len = u16::from_be_bytes([body[at], body[at + 1]]) as usize;
        at += 2;
        if at + len + 1 > body.len() {
            break;
        }
        if let Ok(filter) = std::str::from_utf8(&body[at..at + len]) {
            out.push(filter.to_string());
        }
        at += len + 1; // + the requested QoS byte
    }
    out
}

fn read_u8(stream: &mut TcpStream) -> std::io::Result<u8> {
    let mut buffer = [0u8; 1];
    stream.read_exact(&mut buffer)?;
    Ok(buffer[0])
}

fn read_remaining_length(stream: &mut TcpStream) -> std::io::Result<usize> {
    let mut len = 0usize;
    let mut multiplier = 1usize;
    for _ in 0..4 {
        let byte = read_u8(stream)?;
        len += ((byte & 0x7f) as usize) * multiplier;
        multiplier *= 128;
        if byte & 0x80 == 0 {
            return Ok(len);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "remaining length varint too long",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_matching_follows_the_spec_examples() {
        // MQTT-3.1.1 section 4.7.1 examples (the subset we need).
        assert!(topic_matches("oee/line1/#", "oee/line1/oee"));
        assert!(topic_matches("oee/line1/#", "oee/line1/a/status"));
        assert!(topic_matches("oee/line1/#", "oee/line1"));
        assert!(!topic_matches("oee/line1/#", "oee/line2/oee"));
        assert!(topic_matches("oee/+/status", "oee/line1/status"));
        assert!(!topic_matches("oee/+/status", "oee/line1/oee"));
        assert!(topic_matches("oee/line1/+/end", "oee/line1/a/end"));
        assert!(!topic_matches("oee/line1/+/end", "oee/line1/a/status"));
        assert!(topic_matches("a/b/c", "a/b/c"));
        assert!(!topic_matches("a/b", "a/b/c"));
    }

    #[test]
    fn subscribe_body_filter_parsing() {
        // One filter "t/1" (3 bytes) + qos, packet id 5.
        let body = [0x00, 0x05, 0x00, 0x03, b't', b'/', b'1', 0x00];
        assert_eq!(packet_id(&body), Some(5));
        assert_eq!(filters(&body), vec!["t/1".to_string()]);
        // Truncated bodies parse to nothing instead of panicking.
        assert_eq!(filters(&[0x00, 0x05, 0x00]), Vec::<String>::new());
    }
}
