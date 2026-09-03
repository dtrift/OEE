//! In-process test infrastructure: a loopback MQTT broker.
//!
//! Speaks just enough MQTT 3.1.1 to exercise [`crate::Client`]
//! end-to-end — the offline stand-in for `mosquitto` (D0's
//! `mosquitto_sub` check stays a user action; see the week-4 gate). The
//! `nodes` integration tests reuse this broker through the dev-dependency,
//! so both crates validate the same wire path.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{channel, Receiver};
use std::thread;

/// One captured publish: topic + payload.
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedPublish {
    pub topic: String,
    pub payload: String,
}

/// A spawned loopback broker; `publishes` yields captured PUBLISHes in
/// arrival order. The server thread is detached (process lifetime is the
/// test lifetime; ports are ephemeral).
pub struct LoopbackBroker {
    pub addr: String,
    pub publishes: Receiver<CapturedPublish>,
}

impl LoopbackBroker {
    /// Starts the broker on an ephemeral localhost port.
    pub fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let addr = listener.local_addr().expect("local addr").to_string();
        let (tx, publishes) = channel();
        thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let tx = tx.clone();
                // One thread per connection: nodes hold their connections
                // open concurrently (a sequential serve would serialize
                // them — a broker accepts many clients).
                thread::spawn(move || serve_connection(&mut stream, &tx));
            }
        });
        Self { addr, publishes }
    }
}

/// Serves one connection until EOF: CONNACK on CONNECT, capture PUBLISHes,
/// PINGRESP on PINGREQ.
fn serve_connection(stream: &mut TcpStream, tx: &std::sync::mpsc::Sender<CapturedPublish>) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(30)));
    loop {
        let Some(first) = read_exact_u8(stream) else {
            return;
        };
        let Some(len) = read_remaining_length(stream) else {
            return;
        };
        let mut body = vec![0u8; len];
        if stream.read_exact(&mut body).is_err() {
            return;
        }
        match first {
            0x10 => {
                // CONNECT -> CONNACK(0).
                let _ = stream.write_all(&[0x20, 0x02, 0x00, 0x00]);
            }
            0x30 => {
                // PUBLISH QoS 0: 2-byte topic length + topic + payload.
                if body.len() < 2 {
                    continue;
                }
                let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
                if body.len() < 2 + topic_len {
                    continue;
                }
                let topic = String::from_utf8_lossy(&body[2..2 + topic_len]).into_owned();
                let payload = String::from_utf8_lossy(&body[2 + topic_len..]).into_owned();
                let _ = tx.send(CapturedPublish { topic, payload });
            }
            0xC0 => {
                // PINGREQ -> PINGRESP.
                let _ = stream.write_all(&[0xD0, 0x00]);
            }
            _ => {}
        }
    }
}

fn read_exact_u8(stream: &mut TcpStream) -> Option<u8> {
    let mut buffer = [0u8; 1];
    stream.read_exact(&mut buffer).ok()?;
    Some(buffer[0])
}

fn read_remaining_length(stream: &mut TcpStream) -> Option<usize> {
    let mut len = 0usize;
    let mut multiplier = 1usize;
    for _ in 0..4 {
        let byte = read_exact_u8(stream)?;
        len += ((byte & 0x7f) as usize) * multiplier;
        multiplier *= 128;
        if byte & 0x80 == 0 {
            return Some(len);
        }
    }
    None
}
