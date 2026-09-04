//! The UART→MQTT bridge (the hardware track, phase 2).
//!
//! The boards stream their status lines over USB-CDC; a host terminal
//! pipes them into this bridge (`cat /dev/ttyUSB0 | uart-bridge`), which
//! publishes each line to the bench broker on the `oee/line1/*` topics —
//! the week-5 aggregator/dashboard loop works unmodified, and the boards
//! carry no network code at all.
//!
//! Line → topic mapping (the week-4/5 payload contracts):
//! - `a,<run_id>,<t_ms>,<state>` → `oee/line1/a/status` `{"t_ms":N,"state":"…"}`
//! - `p,<run_id>,<t_ms>,<count>` → `oee/line1/p/count` `{"t_ms":N,"count":N}`
//! - `q,<run_id>,<t_ms>,<verdict>` → `oee/line1/q/verdict` `{"t_ms":N,"verdict":"…"}`
//!
//! Hand-formatted JSON on purpose: the same pinned D3 contract as the
//! aggregator/nodes (no serde on the wire path).

use std::io::{BufRead, Write};

use mqtt_min::Client;

/// The MQTT topic for a parsed node line (`None` — not a status line).
fn topic_for(node: &str) -> Option<&'static str> {
    match node {
        "a" => Some("oee/line1/a/status"),
        "p" => Some("oee/line1/p/count"),
        "q" => Some("oee/line1/q/verdict"),
        _ => None,
    }
}

/// One parsed board line → (topic, payload).
pub fn line_to_publish(line: &str) -> Option<(&'static str, String)> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut fields = line.split(',');
    let node = fields.next()?;
    let _run_id = fields.next()?;
    let t_ms = fields.next()?;
    let value = fields.next()?;
    let t: u64 = t_ms.parse().ok()?;
    let topic = topic_for(node)?;
    let payload = match node {
        "p" => {
            let count: u64 = value.parse().ok()?;
            format!("{{\"t_ms\":{t},\"count\":{count}}}")
        }
        "a" => format!("{{\"t_ms\":{t},\"state\":\"{value}\"}}"),
        "q" => format!("{{\"t_ms\":{t},\"verdict\":\"{value}\"}}"),
        _ => return None,
    };
    Some((topic, payload))
}

/// Publishes the publish markers after the stream ends: each node that was
/// seen gets its `oee/line1/{node}/end` marker (the aggregator's flush
/// contract — the week-5 watermark waits for all expected nodes).
pub fn end_markers(seen: &[&str]) -> Vec<(&'static str, String)> {
    seen.iter()
        .map(|node| (end_topic(node), String::new()))
        .collect()
}

fn end_topic(node: &str) -> &'static str {
    match node {
        "a" => "oee/line1/a/end",
        "p" => "oee/line1/p/end",
        "q" => "oee/line1/q/end",
        _ => "oee/line1/x/end",
    }
}

/// The bridge loop: reads lines from `input`, publishes to the broker,
/// writes the end markers when the input ends. Returns the published count.
pub fn run_bridge<R: BufRead>(
    input: R,
    addr: &str,
    out: &mut impl Write,
) -> std::io::Result<usize> {
    let mut client = Client::connect(addr, "uart-bridge", 60)
        .map_err(|e| std::io::Error::other(format!("mqtt connect {addr}: {e:?}")))?;
    let mut published = 0usize;
    let mut seen = [false; 3]; // a, p, q
    for line in input.lines() {
        let line = line?;
        if let Some((topic, payload)) = line_to_publish(&line) {
            client
                .publish(topic, &payload)
                .map_err(|e| std::io::Error::other(format!("mqtt publish: {e:?}")))?;
            match topic {
                "oee/line1/a/status" => seen[0] = true,
                "oee/line1/p/count" => seen[1] = true,
                "oee/line1/q/verdict" => seen[2] = true,
                _ => {}
            }
            published += 1;
        }
    }
    let nodes: Vec<&str> = ["a", "p", "q"]
        .iter()
        .zip(seen)
        .filter(|(_, s)| *s)
        .map(|(n, _)| *n)
        .collect();
    for (topic, payload) in end_markers(&nodes) {
        client
            .publish(topic, &payload)
            .map_err(|e| std::io::Error::other(format!("mqtt end marker: {e:?}")))?;
        published += 1;
    }
    writeln!(out, "bridge: {published} messages ({})", nodes.join("+")).ok();
    Ok(published)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_three_node_lines() {
        let (topic, payload) = line_to_publish("a,bench-a,1234,run").unwrap();
        assert_eq!(topic, "oee/line1/a/status");
        assert_eq!(payload, r#"{"t_ms":1234,"state":"run"}"#);

        let (topic, payload) = line_to_publish("p,bench-p,2000,131").unwrap();
        assert_eq!(topic, "oee/line1/p/count");
        assert_eq!(payload, r#"{"t_ms":2000,"count":131}"#);

        let (topic, payload) = line_to_publish("q,bench-q,3000,good").unwrap();
        assert_eq!(topic, "oee/line1/q/verdict");
        assert_eq!(payload, r#"{"t_ms":3000,"verdict":"good"}"#);
    }

    #[test]
    fn ignores_non_status_lines() {
        assert!(line_to_publish("a: boot, run_id=bench-a").is_none());
        assert!(line_to_publish("x,run,1,2").is_none());
        assert!(line_to_publish("").is_none());
        assert!(line_to_publish("a,bench-a,NaN,run").is_none());
    }

    #[test]
    fn end_markers_cover_only_seen_nodes() {
        let markers = end_markers(&["a", "q"]);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].0, "oee/line1/a/end");
        assert_eq!(markers[1].0, "oee/line1/q/end");
    }
}
