//! The dashboard state (D3): everything `ui` renders, updated purely from
//! messages — no clocks, no I/O in here (unit-tested headlessly; the MQTT
//! thread feeds it, the render reads it).
//!
//! Zones (plan section 1 reference points): green >= 85%, yellow >= 60%,
//! red below — the world-class / typical / struggling bands.

use std::collections::VecDeque;
use std::time::Instant;

use oee_aggregator::payload::{f32_field, str_field, u32_field};

/// How many verdicts the ticker keeps.
const VERDICT_TICKER: usize = 12;

/// The cumulative (shift) view as published by the aggregator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShiftView {
    pub a: f32,
    pub p: f32,
    pub q: f32,
    pub oee: f32,
    pub parts: u32,
    pub planned_ms: u32,
    pub run_ms: u32,
    pub t_to_ms: u32,
}

impl Default for ShiftView {
    fn default() -> Self {
        // "No data yet" renders as zeros, not NaNs.
        Self {
            a: 0.0,
            p: 0.0,
            q: 0.0,
            oee: 0.0,
            parts: 0,
            planned_ms: 0,
            run_ms: 0,
            t_to_ms: 0,
        }
    }
}

/// Everything the dashboard shows.
#[derive(Debug)]
pub struct DashboardState {
    pub broker_addr: String,
    pub connected: bool,
    /// When the last message of any kind arrived (staleness bar).
    pub last_update: Option<Instant>,
    pub messages: u64,
    /// Payloads that failed to parse (error isolation: garbage never takes
    /// the display down, it is counted instead).
    pub parse_errors: u64,
    /// Current machine status from `a/status` (idle/run/jam/overload).
    pub a_state: Option<String>,
    /// Cumulative part counter from `p/count`.
    pub count: Option<u32>,
    /// The latest verdicts from `q/verdict`, newest last.
    pub verdicts: VecDeque<String>,
    /// The latest cumulative OEE row from `oee/line1/oee`.
    pub shift: ShiftView,
    /// Minute-window OEE history, percent (the sparkline).
    pub history: Vec<u64>,
    /// The run id seen last.
    pub run_id: Option<String>,
    /// Set when any node's stream-end marker arrived.
    pub finished: bool,
}

impl DashboardState {
    pub fn new(broker_addr: &str) -> Self {
        Self {
            broker_addr: broker_addr.to_string(),
            connected: false,
            last_update: None,
            messages: 0,
            parse_errors: 0,
            a_state: None,
            count: None,
            verdicts: VecDeque::new(),
            shift: ShiftView::default(),
            history: Vec::new(),
            run_id: None,
            finished: false,
        }
    }

    /// Feeds one message (full topic, e.g. `oee/line1/oee`); `now` comes
    /// from the caller (testable clocks). Unknown topics and corrupt
    /// payloads are counted, never fatal.
    pub fn on_message(&mut self, topic: &str, payload: &str, now: Instant) {
        self.messages += 1;
        self.last_update = Some(now);
        if let Some(run_id) = str_field(payload, "run_id") {
            self.run_id = Some(run_id.to_string());
        }
        let Some(suffix) = topic.strip_prefix("oee/line1/") else {
            return; // a foreign topic on the same broker: ignored
        };
        match suffix {
            "a/status" => match str_field(payload, "state") {
                Some(state) => self.a_state = Some(state.to_string()),
                None => self.parse_errors += 1,
            },
            "p/count" => match u32_field(payload, "count") {
                Some(count) => self.count = Some(count),
                None => self.parse_errors += 1,
            },
            "q/verdict" => match str_field(payload, "verdict") {
                Some(verdict) => {
                    self.verdicts.push_back(verdict.to_string());
                    if self.verdicts.len() > VERDICT_TICKER {
                        self.verdicts.pop_front();
                    }
                }
                None => self.parse_errors += 1,
            },
            "oee" => self.on_oee_payload(payload),
            other => {
                if other.ends_with("/end") {
                    self.finished = true;
                }
                // Meta topics and everything else: counted as traffic only.
            }
        }
    }

    /// Parses `oee/line1/oee` payloads (the pinned D2 schema): `shift`
    /// updates the gauges, `minute` appends to the sparkline history.
    fn on_oee_payload(&mut self, payload: &str) {
        let scope = str_field(payload, "scope");
        match scope {
            Some("shift") => {
                let (a, p, q, oee) = (
                    f32_field(payload, "a"),
                    f32_field(payload, "p"),
                    f32_field(payload, "q"),
                    f32_field(payload, "oee"),
                );
                if let (Some(a), Some(p), Some(q), Some(oee)) = (a, p, q, oee) {
                    self.shift = ShiftView {
                        a,
                        p,
                        q,
                        oee,
                        parts: u32_field(payload, "parts").unwrap_or(0),
                        planned_ms: u32_field(payload, "planned_ms").unwrap_or(0),
                        run_ms: u32_field(payload, "run_ms").unwrap_or(0),
                        t_to_ms: u32_field(payload, "t_to_ms").unwrap_or(0),
                    };
                } else {
                    self.parse_errors += 1;
                }
            }
            Some("minute") => {
                if let Some(oee) = f32_field(payload, "oee") {
                    self.history.push((oee.clamp(0.0, 1.0) * 1000.0) as u64);
                } else {
                    self.parse_errors += 1;
                }
            }
            _ => self.parse_errors += 1,
        }
    }
}

/// The zone color of a component value: green >= 85%, yellow >= 60% (plan
/// section 1 reference points), red below.
pub fn zone(value: f32) -> Zone {
    if value >= 0.85 {
        Zone::Green
    } else if value >= 0.60 {
        Zone::Yellow
    } else {
        Zone::Red
    }
}

/// A performance zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Green,
    Yellow,
    Red,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_topics_fill_the_view() {
        let mut state = DashboardState::new("127.0.0.1:1883");
        let now = Instant::now();
        state.on_message(
            "oee/line1/a/status",
            r#"{"state":"run","t_ms":2159,"run_id":"normal-42"}"#,
            now,
        );
        state.on_message(
            "oee/line1/p/count",
            r#"{"count":7,"t_ms":4200,"run_id":"normal-42"}"#,
            now,
        );
        state.on_message(
            "oee/line1/q/verdict",
            r#"{"verdict":"good","t_ms":4000,"run_id":"normal-42"}"#,
            now,
        );
        state.on_message(
            "oee/line1/q/verdict",
            r#"{"verdict":"cracked","t_ms":4400,"run_id":"normal-42"}"#,
            now,
        );
        state.on_message(
            "oee/line1/oee",
            r#"{"scope":"shift","run_id":"normal-42","t_from_ms":0,"t_to_ms":60000,"planned_ms":60000,"run_ms":50400,"parts":126,"good":88,"total":126,"a":0.840,"p":1.000,"q":0.698,"oee":0.586}"#,
            now,
        );
        state.on_message(
            "oee/line1/oee",
            r#"{"scope":"minute","run_id":"normal-42","t_from_ms":0,"t_to_ms":60000,"planned_ms":60000,"run_ms":50400,"parts":126,"good":88,"total":126,"a":0.840,"p":1.000,"q":0.698,"oee":0.586}"#,
            now,
        );

        assert_eq!(state.a_state.as_deref(), Some("run"));
        assert_eq!(state.count, Some(7));
        assert_eq!(state.verdicts, ["good", "cracked"]);
        assert_eq!(state.run_id.as_deref(), Some("normal-42"));
        assert_eq!(state.messages, 6);
        assert_eq!(state.parse_errors, 0);
        let shift = state.shift;
        assert!((shift.a - 0.840).abs() < 1e-3);
        assert!((shift.oee - 0.586).abs() < 1e-3);
        assert_eq!(shift.parts, 126);
        assert_eq!(state.history, vec![586]);
        assert!(!state.finished);
        // A node end marker flips the finished flag.
        state.on_message("oee/line1/a/end", r#"{"t_ms":59999}"#, now);
        assert!(state.finished);
    }

    #[test]
    fn corrupt_payloads_never_take_the_display_down() {
        // The D3 error-isolation test: garbage on every topic is counted,
        // the state keeps whatever it had, nothing panics.
        let mut state = DashboardState::new("127.0.0.1:1883");
        let now = Instant::now();
        for topic in ["oee/line1/a/status", "oee/line1/p/count", "oee/line1/oee"] {
            state.on_message(topic, "", now);
            state.on_message(topic, "{not json", now);
            state.on_message(topic, r#"{"scope":"shift","a":}"#, now);
            state.on_message(topic, " garbage with \x00 bytes ", now);
        }
        assert_eq!(state.parse_errors, 12, "four corrupt payloads per topic");
        assert_eq!(state.messages, 12);
        assert_eq!(state.a_state, None);
        assert_eq!(state.count, None);
        assert_eq!(state.shift.oee, 0.0, "the last good view stays");
        assert!(state.history.is_empty());
        // And the display still accepts good data afterwards.
        state.on_message(
            "oee/line1/p/count",
            r#"{"count":1,"t_ms":100,"run_id":"r"}"#,
            now,
        );
        assert_eq!(state.count, Some(1));
    }

    #[test]
    fn verdict_ticker_is_bounded() {
        let mut state = DashboardState::new("x");
        let now = Instant::now();
        for i in 0..40 {
            state.on_message(
                "oee/line1/q/verdict",
                &format!(r#"{{"verdict":"good","t_ms":{i}}}"#),
                now,
            );
        }
        assert_eq!(state.verdicts.len(), VERDICT_TICKER);
        assert_eq!(state.verdicts.back().map(String::as_str), Some("good"));
    }

    #[test]
    fn zones_follow_the_plan_thresholds() {
        assert_eq!(zone(0.85), Zone::Green);
        assert_eq!(zone(1.0), Zone::Green);
        assert_eq!(zone(0.60), Zone::Yellow);
        assert_eq!(zone(0.84), Zone::Yellow);
        assert_eq!(zone(0.599), Zone::Red);
        assert_eq!(zone(0.0), Zone::Red);
    }

    #[test]
    fn foreign_topics_are_traffic_only() {
        let mut state = DashboardState::new("x");
        let now = Instant::now();
        state.on_message("other/line/x", r#"{"count":9}"#, now);
        state.on_message("oee/line1/a/meta", r#"{"model":"x"}"#, now);
        assert_eq!(state.messages, 2);
        assert_eq!(state.parse_errors, 0);
        assert_eq!(state.count, None, "a foreign count is ignored");
    }
}
