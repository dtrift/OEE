//! A minimal MQTT 3.1.1 client subset over `std::net::TcpStream` (week 4,
//! D0/D2 deviation): CONNECT/PUBLISH (QoS 0)/PINGREQ — exactly what the OEE
//! nodes need to publish statuses, nothing more.
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
//! - PINGREQ `0xC0 0x00` / PINGRESP `0xD0 0x00`;
//! - remaining length: base-128 varint, up to 4 bytes.

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

/// A connected client (QoS 0 publishing only).
pub struct Client {
    stream: TcpStream,
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
        Ok(Self { stream })
    }

    /// Publishes a payload (QoS 0, no acknowledgement — fire and forget).
    pub fn publish(&mut self, topic: &str, payload: &str) -> Result<(), MqttError> {
        let mut packet = Vec::new();
        write_utf8(&mut packet, topic);
        packet.extend_from_slice(payload.as_bytes());
        write_packet(&mut self.stream, 0x30, &packet)
    }

    /// Keepalive ping (PINGREQ/PINGRESP round-trip).
    pub fn ping(&mut self) -> Result<(), MqttError> {
        write_packet(&mut self.stream, 0xC0, &[])?;
        let (kind, _) = read_packet(&mut self.stream)?;
        if kind != 0xD0 {
            return Err(MqttError::Protocol("expected PINGRESP"));
        }
        Ok(())
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

/// Reads one packet: returns `(type_and_flags, body)`.
fn read_packet(stream: &mut TcpStream) -> Result<(u8, Vec<u8>), MqttError> {
    let mut first = [0u8; 1];
    stream.read_exact(&mut first)?;
    let kind = first[0];
    let (len, _) = read_remaining_length(stream)?;
    let mut body = vec![0u8; len];
    stream.read_exact(&mut body)?;
    Ok((kind, body))
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
}
