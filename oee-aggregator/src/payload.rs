//! The `oee/line1/oee` payload contract (week 5, D2/D3): a fixed-shape,
//! escape-free JSON built by hand (the `mqtt_sink` convention — no
//! serde_json dependency) and a tiny scanner on the consumer side.
//!
//! Producer side: [`oee_payload`] — keys in a pinned order, floats with 3
//! decimals. Consumer side: [`str_field`] / [`u32_field`] / [`f32_field`] —
//! substring scans that return `None` on anything unexpected (corrupt
//! payloads must never take down the dashboard; the caller counts them).
//!
//! Pinned shape (the D3 dashboard contract):
//!
//! ```text
//! {"scope":"minute","run_id":"normal-42","t_from_ms":0,"t_to_ms":60000,
//!  "planned_ms":60000,"run_ms":50400,"parts":126,"good":88,"total":126,
//!  "a":0.840,"p":1.000,"q":0.698,"oee":0.586}
//! ```
//!
//! `scope` is `"minute"` (one aggregation window) or `"shift"` (cumulative
//! since run start — the bench-run stand-in for a shift).

use crate::windows::WindowStats;

/// Builds the payload for one computed window.
pub fn oee_payload(scope: &str, run_id: &str, stats: &WindowStats) -> String {
    format!(
        concat!(
            r#"{{"scope":"{scope}","run_id":"{run_id}","t_from_ms":{t_from},"t_to_ms":{t_to},"#,
            r#""planned_ms":{planned},"run_ms":{run},"parts":{parts},"#,
            r#""good":{good},"total":{total},"a":{a:.3},"p":{p:.3},"q":{q:.3},"oee":{oee:.3}}}"#
        ),
        scope = scope,
        run_id = run_id,
        t_from = stats.t_from_ms,
        t_to = stats.t_to_ms,
        planned = stats.planned_ms,
        run = stats.run_ms,
        parts = stats.parts,
        good = stats.good,
        total = stats.total,
        a = stats.availability,
        p = stats.performance,
        q = stats.quality,
        oee = stats.oee,
    )
}

/// The value of `"key":"…"` (a string field), if present and well-formed.
pub fn str_field<'a>(payload: &'a str, key: &str) -> Option<&'a str> {
    let at = find_key(payload, key)?;
    let rest = payload[at..].trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The value of `"key":<uint>` if present and well-formed.
pub fn u32_field(payload: &str, key: &str) -> Option<u32> {
    number_text(payload, key)?.parse().ok()
}

/// The value of `"key":<float>` if present and well-formed.
pub fn f32_field(payload: &str, key: &str) -> Option<f32> {
    number_text(payload, key)?.parse().ok()
}

/// The raw numeric text after `"key":` (digits and `- . e E +`).
fn number_text<'a>(payload: &'a str, key: &str) -> Option<&'a str> {
    let at = find_key(payload, key)?;
    let rest = payload[at..].trim_start();
    let end = rest
        .find(|c: char| !matches!(c, '-' | '+' | '.' | 'e' | 'E' | '0'..='9'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// The byte offset just past `"key":` (escaped quotes are never produced by
/// our payloads; a needle with one is simply absent).
fn find_key(payload: &str, key: &str) -> Option<usize> {
    let mut needle = String::with_capacity(key.len() + 3);
    needle.push('"');
    needle.push_str(key);
    needle.push_str("\":");
    let at = payload.find(&needle)? + needle.len();
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> WindowStats {
        WindowStats {
            t_from_ms: 0,
            t_to_ms: 60_000,
            planned_ms: 60_000,
            run_ms: 50_400,
            parts: 126,
            good: 88,
            total: 126,
            availability: 0.84,
            performance: 1.0,
            quality: 88.0 / 126.0,
            oee: 0.84 * 1.0 * 88.0 / 126.0,
        }
    }

    #[test]
    fn payload_shape_is_pinned() {
        let payload = oee_payload("minute", "normal-42", &stats());
        assert!(payload.starts_with(r#"{"scope":"minute","run_id":"normal-42""#));
        assert!(payload.contains(r#""t_from_ms":0,"t_to_ms":60000"#));
        assert!(payload.contains(r#""planned_ms":60000,"run_ms":50400"#));
        assert!(payload.contains(r#""parts":126,"good":88,"total":126"#));
        assert!(payload.contains(r#""a":0.840,"p":1.000,"q":0.698"#));
        assert!(payload.ends_with(r#""oee":0.587}"#));
    }

    #[test]
    fn fields_round_trip() {
        let payload = oee_payload("shift", "run1", &stats());
        assert_eq!(str_field(&payload, "scope"), Some("shift"));
        assert_eq!(str_field(&payload, "run_id"), Some("run1"));
        assert_eq!(u32_field(&payload, "t_to_ms"), Some(60_000));
        assert_eq!(u32_field(&payload, "parts"), Some(126));
        assert!((f32_field(&payload, "a").unwrap() - 0.840).abs() < 1e-4);
        assert!((f32_field(&payload, "q").unwrap() - 0.698).abs() < 1e-3);
    }

    #[test]
    fn scans_survive_neighbouring_keys() {
        // "t_from_ms" must not be confused with "from_ms"; "a" must not
        // match the "a" inside "run_id"'s value or "planned_ms".
        let payload = oee_payload("minute", "alpha", &stats());
        assert_eq!(u32_field(&payload, "t_from_ms"), Some(0));
        assert_eq!(u32_field(&payload, "from_ms"), None);
        assert!((f32_field(&payload, "a").unwrap() - 0.84).abs() < 1e-3);
        assert_eq!(f32_field(&payload, "run"), None);
    }

    #[test]
    fn corrupt_payloads_return_none_without_panicking() {
        // The dashboard error-isolation contract: garbage in, None out.
        for corrupt in [
            "",
            "{",
            "not json at all",
            r#"{"scope":}"#,
            r#"{"parts":"not a number"}"#,
            r#"{"t_to_ms":-}"#,
            r#"{"run_id":"unterminated}"#,
        ] {
            assert_eq!(str_field(corrupt, "scope"), None, "input: {corrupt:?}");
            assert_eq!(u32_field(corrupt, "parts"), None, "input: {corrupt:?}");
            assert_eq!(f32_field(corrupt, "a"), None, "input: {corrupt:?}");
        }
    }

    #[test]
    fn node_payloads_parse_with_the_same_scanner() {
        // The aggregator reads node payloads with these helpers too.
        let status = r#"{"state":"run","t_ms":2159,"run_id":"normal-42"}"#;
        assert_eq!(str_field(status, "state"), Some("run"));
        assert_eq!(u32_field(status, "t_ms"), Some(2159));
        let count = r#"{"count":7,"t_ms":4200,"run_id":"normal-42"}"#;
        assert_eq!(u32_field(count, "count"), Some(7));
        let end = r#"{"t_ms":59999,"run_id":"normal-42"}"#;
        assert_eq!(u32_field(end, "t_ms"), Some(59_999));
    }
}
