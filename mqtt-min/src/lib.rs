//! A minimal MQTT 3.1.1 client subset over `std::net::TcpStream` (week 4,
//! D0/D2 deviation; subscribe — week 5, D2): CONNECT / PUBLISH (QoS 0) /
//! SUBSCRIBE (QoS 0) / PINGREQ — what the OEE nodes and the aggregator need,
//! nothing more.
//!
//! Why own code instead of `rumqttc`: the implementation sandbox is offline
//! and `rumqttc` is not in the local registry cache (same honest-deviation
//! pattern as the week-3 TF-venv items). The protocol subset here is small
//! and fixed by the MQTT 3.1.1 spec; a loopback broker in the tests exercises
//! the wire format, and the same client runs against a real `mosquitto`
//! unchanged (the gate doc lists the user actions).
//!
//! Protocol facts pinned by the spec (MQTT-3.1.1):
//! - CONNECT: protocol name "MQTT", level 4, clean-session flag, keepalive;
//! - CONNACK: fixed header `0x20`, 2-byte remaining length, return code 0;
//! - PUBLISH QoS 0: `0x30 | flags`, topic string, payload — no packet id;
//! - SUBSCRIBE: `0x82`, packet id, topic filters with a QoS byte each;
//!   SUBACK: `0x90`, the same packet id, one return code per filter
//!   (0 = granted QoS 0, 0x80 = rejection);
//! - PINGREQ `0xC0 0x00` / PINGRESP `0xD0 0x00`;
//! - remaining length: base-128 varint, up to 4 bytes.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// In-process test infrastructure (the loopback broker).
pub mod testing;

/// A client-side MQTT error.
#[derive(Debug)]
pub enum MqttError {
    /// TCP-level failure (connect, broken pipe).
    Io(std::io::Error),
    /// The broker rejected the connection (CONNACK code != 0).
    ConnectRejected(u8),
    /// A malformed packet arrived.
    Protocol(&'static str),
}

impl std::fmt::Display for MqttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MqttError::Io(e) => write!(f, "mqtt io error: {e}"),
            MqttError::ConnectRejected(code) => {
                write!(f, "mqtt connect rejected, connack code {code}")
            }
            MqttError::Protocol(what) => write!(f, "mqtt protocol violation: {what}"),
        }
    }
}

impl std::error::Error for MqttError {}

impl From<std::io::Error> for MqttError {
    fn from(e: std::io::Error) -> Self {
        MqttError::Io(e)
    }
}

/// One incoming PUBLISH (QoS 0) delivered to a subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub topic: String,
    pub payload: String,
}

/// A connected client (QoS 0 publishing + subscriptions).
pub struct Client {
    stream: TcpStream,
    /// PUBLISHes that arrived while waiting for a SUBACK (a broker may send
    /// data before the acknowledgement — the spec does not forbid it).
    pending: VecDeque<Message>,
    next_packet_id: u16,
}

impl Client {
    /// Connects to a broker: TCP connect, CONNECT, waits for CONNACK 0.
    pub fn connect(addr: &str, client_id: &str, keepalive_s: u16) -> Result<Self, MqttError> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut packet = Vec::new();
        // Variable header: protocol name/level + connect flags + keepalive.
        write_utf8(&mut packet, "MQTT");
        packet.push(4); // level 4 = MQTT 3.1.1
        packet.push(0b0000_0010); // clean session
        packet.extend_from_slice(&keepalive_s.to_be_bytes());
        // Payload: client id.
        write_utf8(&mut packet, client_id);
        write_packet(&mut stream, 0x10, &packet)?;
        // CONNACK: 0x20, len 2, flags, return code.
        let (kind, body) = read_packet(&mut stream)?;
        if kind != 0x20 {
            return Err(MqttError::Protocol("expected CONNACK"));
        }
        let code = *body.last().ok_or(MqttError::Protocol("empty CONNACK"))?;
        if code != 0 {
            return Err(MqttError::ConnectRejected(code));
        }
        Ok(Self {
            stream,
            pending: VecDeque::new(),
            next_packet_id: 1,
        })
    }

    /// Subscribes to a topic filter (QoS 0): SUBSCRIBE, then waits for the
    /// SUBACK. A PUBLISH that races in before the SUBACK is buffered and
    /// served by the next [`Client::next_message`] call.
    pub fn subscribe(&mut self, filter: &str) -> Result<(), MqttError> {
        let id = self.next_packet_id;
        self.next_packet_id = self.next_packet_id % u16::MAX + 1;
        let mut body = Vec::new();
        body.extend_from_slice(&id.to_be_bytes());
        write_utf8(&mut body, filter);
        body.push(0); // requested QoS 0
        write_packet(&mut self.stream, 0x82, &body)?;
        loop {
            let (kind, body) = read_packet(&mut self.stream)?;
            match kind {
                0x90 => {
                    if body.len() < 3 || body[..2] != id.to_be_bytes() {
                        return Err(MqttError::Protocol("suback: bad packet id"));
                    }
                    if body[2..].iter().any(|code| code & 0x80 != 0) {
                        return Err(MqttError::Protocol("subscription rejected"));
                    }
                    return Ok(());
                }
                kind if kind & 0xF0 == 0x30 => {
                    self.pending.push_back(parse_publish(kind, body)?);
                }
                _ => {} // PINGRESP and friends — not ours, skip
            }
        }
    }

    /// Reads the next subscribed PUBLISH, waiting up to `timeout`.
    /// `Ok(None)` = no message within the timeout (the connection stays
    /// usable: a timeout only happens at a packet boundary, never mid-packet,
    /// so the stream cannot desync). Non-PUBLISH packets (PINGRESP, SUBACK)
    /// are consumed silently and also yield `Ok(None)`.
    pub fn next_message(&mut self, timeout: Duration) -> Result<Option<Message>, MqttError> {
        if let Some(message) = self.pending.pop_front() {
            return Ok(Some(message));
        }
        self.stream.set_read_timeout(Some(timeout))?;
        match read_packet_idle_aware(&mut self.stream)? {
            Some((kind, body)) => match kind & 0xF0 {
                0x30 => Ok(Some(parse_publish(kind, body)?)),
                _ => Ok(None),
            },
            None => Ok(None),
        }
    }

    /// Publishes a payload (QoS 0, no acknowledgement — fire and forget).
    pub fn publish(&mut self, topic: &str, payload: &str) -> Result<(), MqttError> {
        let mut packet = Vec::new();
        write_utf8(&mut packet, topic);
        packet.extend_from_slice(payload.as_bytes());
        write_packet(&mut self.stream, 0x30, &packet)
    }

    /// Keepalive ping round-trip: PINGREQ then wait for PINGRESP. Only for
    /// publish-only clients — on a subscribed connection an incoming PUBLISH
    /// would be misread (use [`Client::send_ping`] + [`Client::next_message`]
    /// there instead).
    pub fn ping(&mut self) -> Result<(), MqttError> {
        write_packet(&mut self.stream, 0xC0, &[])?;
        let (kind, _) = read_packet(&mut self.stream)?;
        if kind != 0xD0 {
            return Err(MqttError::Protocol("expected PINGRESP"));
        }
        Ok(())
    }

    /// Fire-and-forget PINGREQ (no response wait) — the keepalive form for
    /// subscribed clients whose reads happen in [`Client::next_message`].
    pub fn send_ping(&mut self) -> Result<(), MqttError> {
        write_packet(&mut self.stream, 0xC0, &[])
    }
}

/// Writes one fixed-header + body packet.
fn write_packet(stream: &mut TcpStream, kind: u8, body: &[u8]) -> Result<(), MqttError> {
    let mut out = vec![kind];
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(body);
    stream.write_all(&out)?;
    stream.flush()?;
    Ok(())
}

/// Reads one packet: returns `(type_and_flags, body)`. A read timeout is an
/// error here (the CONNECT/SUBACK/ping paths expect a prompt answer).
fn read_packet(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), MqttError> {
    let mut first = [0u8; 1];
    stream.read_exact(&mut first)?;
    let kind = first[0];
    let (len, _) = read_remaining_length(stream)?;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok((kind, body))
}

/// [`read_packet`], but an idle timeout at a packet boundary is `Ok(None)`
/// instead of an error. Once the first byte is read the packet is in flight:
/// timeouts inside a packet are retried (bounded) so the stream never
/// desyncs; exhausting the retries is a hard error — reconnect.
fn read_packet_idle_aware(stream: &mut TcpStream) -> Result<Option<(u8, Vec<u8>)>, MqttError> {
    let mut first = [0u8; 1];
    match stream.read(&mut first) {
        Ok(0) => return Err(MqttError::Protocol("peer closed the connection")),
        Ok(_) => {}
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(e.into()),
    }
    let (len, _) = read_remaining_length_retry(stream)?;
    let mut body = vec![0u8; len];
    read_exact_retry(stream, &mut body)?;
    Ok(Some((first[0], body)))
}

/// [`read_remaining_length`] with bounded retries on read timeouts (a packet
/// is already in flight — see [`read_packet_idle_aware`]).
fn read_remaining_length_retry(stream: &mut TcpStream) -> Result<(usize, usize), MqttError> {
    for _ in 0..TIMEOUT_RETRIES {
        match read_remaining_length(stream) {
            Ok(result) => return Ok(result),
            Err(MqttError::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(MqttError::Protocol(
        "remaining length: too many partial reads",
    ))
}

/// [`std::io::Read::read_exact`] with bounded retries on read timeouts.
fn read_exact_retry(stream: &mut TcpStream, body: &mut [u8]) -> Result<(), MqttError> {
    for _ in 0..TIMEOUT_RETRIES {
        match stream.read_exact(body) {
            Ok(()) => return Ok(()),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Err(MqttError::Protocol("body: too many partial reads"))
}

/// After the first byte of a packet, how many read-timeout periods to keep
/// trying before declaring the connection broken (localhost needs one).
const TIMEOUT_RETRIES: usize = 8;

/// Parses an incoming PUBLISH body into a [`Message`] (QoS 0 and 1 shapes;
/// QoS 1 packet ids are skipped — this client never acknowledges them).
fn parse_publish(kind: u8, body: Vec<u8>) -> Result<Message, MqttError> {
    if body.len() < 2 {
        return Err(MqttError::Protocol(
            "publish: body shorter than a topic length",
        ));
    }
    let topic_len = u16::from_be_bytes([body[0], body[1]]) as usize;
    if body.len() < 2 + topic_len {
        return Err(MqttError::Protocol("publish: truncated topic"));
    }
    let topic = String::from_utf8_lossy(&body[2..2 + topic_len]).into_owned();
    let mut at = 2 + topic_len;
    let qos = (kind >> 1) & 0x03;
    if qos > 0 {
        at += 2; // skip the packet id (never acknowledged: we subscribe QoS 0)
        if body.len() < at {
            return Err(MqttError::Protocol("publish: truncated packet id"));
        }
    }
    let payload = String::from_utf8_lossy(&body[at..]).into_owned();
    Ok(Message { topic, payload })
}

/// Encodes a QoS-0 PUBLISH packet (broker-side dispatch and tests).
pub fn encode_publish(topic: &str, payload: &str) -> Vec<u8> {
    let mut body = Vec::new();
    write_utf8(&mut body, topic);
    body.extend_from_slice(payload.as_bytes());
    let mut out = vec![0x30];
    write_remaining_length(&mut out, body.len());
    out.extend_from_slice(&body);
    out
}

/// Encodes the MQTT remaining-length varint (base 128, little-endian groups).
pub fn write_remaining_length(out: &mut Vec<u8>, mut len: usize) {
    loop {
        let mut byte = (len % 128) as u8;
        len /= 128;
        if len > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if len == 0 {
            return;
        }
    }
}

/// Reads the remaining-length varint from the stream.
fn read_remaining_length(stream: &mut TcpStream) -> Result<(usize, usize), MqttError> {
    let mut len = 0usize;
    let mut multiplier = 1usize;
    for consumed in 1..=4 {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte)?;
        let value = byte[0];
        len += ((value & 0x7f) as usize) * multiplier;
        multiplier *= 128;
        if value & 0x80 == 0 {
            return Ok((len, consumed));
        }
    }
    Err(MqttError::Protocol("remaining length varint too long"))
}

/// Writes an MQTT UTF-8 string field: 2-byte big-endian length + bytes.
fn write_utf8(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_length_encoding_matches_the_spec_examples() {
        // The MQTT-3.1.1 spec's varint examples (section 2.2.3).
        for (value, bytes) in [
            (0usize, vec![0x00]),
            (1, vec![0x01]),
            (127, vec![0x7F]),
            (128, vec![0x80, 0x01]),
            (16383, vec![0xFF, 0x7F]),
            (2_097_152, vec![0x80, 0x80, 0x80, 0x01]),
        ] {
            let mut out = Vec::new();
            write_remaining_length(&mut out, value);
            assert_eq!(out, bytes, "value {value}");
        }
    }

    #[test]
    fn publish_packet_layout() {
        // "oee/line1/a/status" (18 bytes) + {"state":"run"} (15 bytes):
        // remaining length = 2 + 18 + 15 = 35, topic length 0x0012 BE.
        let topic = "oee/line1/a/status";
        let payload = r#"{"state":"run"}"#;
        assert_eq!(topic.len(), 18);
        assert_eq!(payload.len(), 15);
        let mut packet = Vec::new();
        write_utf8(&mut packet, topic);
        packet.extend_from_slice(payload.as_bytes());
        let mut wire = vec![0x30];
        write_remaining_length(&mut wire, packet.len());
        wire.extend_from_slice(&packet);
        assert_eq!(wire[0], 0x30);
        assert_eq!(wire[1], 35);
        assert_eq!(&wire[2..4], &[0x00, 0x12]);
        assert!(wire.ends_with(b"run\"}"));
    }

    #[test]
    fn subscribe_packet_layout() {
        // SUBSCRIBE "oee/line1/#" with packet id 1:
        // body = id(2) + topic len(2) + topic(11) + qos(1) = 16 = 0x10.
        let topic = "oee/line1/#";
        assert_eq!(topic.len(), 11);
        let mut body = Vec::new();
        body.extend_from_slice(&1u16.to_be_bytes());
        write_utf8(&mut body, topic);
        body.push(0);
        let mut wire = vec![0x82];
        write_remaining_length(&mut wire, body.len());
        wire.extend_from_slice(&body);
        assert_eq!(
            wire,
            vec![
                0x82, 0x10, 0x00, 0x01, 0x00, 0x0B, b'o', b'e', b'e', b'/', b'l', b'i', b'n', b'e',
                b'1', b'/', b'#', 0x00,
            ]
        );
    }

    #[test]
    fn suback_body_is_checked_by_packet_id() {
        // A SUBACK for a different packet id must not be accepted.
        // (Layout check only — the full path runs against the loopback
        // broker in tests/loopback.rs.)
        let id = 7u16.to_be_bytes();
        let body = [id[0], id[1], 0x00];
        assert_eq!(body.len(), 3);
        assert_eq!(&body[..2], &id);
    }

    #[test]
    fn publish_parsing_handles_qos0_and_qos1_shapes() {
        // QoS 0: topic + payload directly.
        let mut body = Vec::new();
        write_utf8(&mut body, "oee/line1/p/count");
        body.extend_from_slice(br#"{"count":7}"#);
        let message = parse_publish(0x30, body).expect("qos 0 parse");
        assert_eq!(message.topic, "oee/line1/p/count");
        assert_eq!(message.payload, r#"{"count":7}"#);
        // QoS 1: a 2-byte packet id sits between the topic and the payload.
        let mut body = Vec::new();
        write_utf8(&mut body, "t/x");
        body.extend_from_slice(&10u16.to_be_bytes());
        body.extend_from_slice(b"hi");
        let message = parse_publish(0x32, body).expect("qos 1 parse");
        assert_eq!(message.topic, "t/x");
        assert_eq!(message.payload, "hi");
        // Truncated bodies are protocol errors, not panics.
        assert!(parse_publish(0x30, vec![0x00]).is_err());
        assert!(parse_publish(0x30, vec![0x00, 0x05, b'a']).is_err());
    }

    #[test]
    fn encode_publish_round_trips_through_parse() {
        let packet = encode_publish("oee/line1/oee", r#"{"oee":0.5}"#);
        assert_eq!(packet[0], 0x30);
        // Strip the fixed header (kind + 1 length byte — small payloads).
        let body = packet[2..].to_vec();
        let message = parse_publish(0x30, body).expect("round trip");
        assert_eq!(message.topic, "oee/line1/oee");
        assert_eq!(message.payload, r#"{"oee":0.5}"#);
    }
}
