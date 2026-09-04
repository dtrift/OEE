//! The aggregator runtime (week 5, D2): subscribes to the node topics,
//! folds the streams into minute windows + a cumulative shift view, and
//! publishes `oee/line1/oee`.
//!
//! **Event-time, not wall-clock.** The nodes replay recorded runs as fast
//! as they parse — MQTT messages arrive in bursts, not real time. Every
//! window boundary therefore comes from the payloads' `t_ms` (machine
//! time), and a **watermark** makes the fold deterministic despite the
//! arbitrary interleaving of the three publisher threads:
//!
//! - per source (a/p/q) messages arrive in stream order (one TCP
//!   connection each — order is guaranteed end-to-end);
//! - the watermark is `min(last seen t_ms of every not-yet-ended source)`;
//!   a minute window closes only once the watermark has passed its end, so
//!   every event below the boundary is already in, regardless of who raced
//!   whom;
//! - each node publishes `oee/line1/{node}/end` (with its last stream
//!   time) at stream end; once every expected node has ended, the final
//!   (possibly partial) window is flushed and the run returns.
//!
//! Published rows: one `minute` payload per closed window plus the final
//! partial one, and `shift` payloads — cumulative since run start — on
//! every window close and as a live snapshot after every processed
//! message. The CSV log records window closes only (minute + shift), which
//! makes it bit-identical across runs with the same seed (the week-5
//! determinism check reads exactly this file).

use std::io::Write;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use mqtt_min::Client;

use crate::payload::{oee_payload, str_field, u32_field};
use crate::windows::{compute, WindowInputs, WindowStats};

/// Topic layout relative to the configured prefix (default `oee/line1`).
pub mod topics {
    pub const A_STATUS: &str = "a/status";
    pub const P_COUNT: &str = "p/count";
    pub const Q_VERDICT: &str = "q/verdict";
    /// The OEE output of this aggregator (the dashboard's input).
    pub const OEE: &str = "oee";
    /// Node stream-end markers: `{node}/end` (subscribed via `+/end`).
    pub const END_FILTER: &str = "+/end";
}

/// The three sources, in watermark order.
const SOURCES: [&str; 3] = ["a", "p", "q"];

/// Aggregator configuration.
#[derive(Debug, Clone)]
pub struct Config {
    /// Broker address, `host:port`.
    pub broker_addr: String,
    /// Topic prefix (default `oee/line1`).
    pub topic_prefix: String,
    /// Nominal line cadence — the ideal cycle time of the P formula. A line
    /// property, not a scenario parameter: a slowdown scenario keeps the
    /// nominal ideal.
    pub ideal_cycle_ms: u32,
    /// The minute-window length, ms of machine time.
    pub minute_ms: u32,
    /// Which node streams to expect (any subset of `a`, `p`, `q`); the
    /// final flush waits for exactly these end markers.
    pub expect_nodes: Vec<String>,
    /// Where to write the windows CSV; skipped when `None` (tests).
    pub csv_path: Option<std::path::PathBuf>,
    /// Signalled once the subscriptions are in place (the experiment starts
    /// the nodes only after this — QoS 0 does not replay missed messages).
    pub ready: Option<Sender<()>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            broker_addr: "127.0.0.1:1883".into(),
            topic_prefix: "oee/line1".into(),
            ideal_cycle_ms: 400,
            minute_ms: 60_000,
            expect_nodes: vec!["a".into(), "p".into(), "q".into()],
            csv_path: None,
            ready: None,
        }
    }
}

/// The aggregator run summary.
#[derive(Debug, Default, PartialEq)]
pub struct RunSummary {
    /// Messages consumed.
    pub messages: usize,
    /// Node payloads that failed to parse (error isolation counter).
    pub parse_errors: usize,
    /// Closed minute windows (including the final partial one).
    pub windows: usize,
    /// Rows published (minute + shift + live shift snapshots).
    pub publishes: usize,
    /// The final cumulative (shift) row — the measured OEE of the run.
    pub final_shift: Option<WindowStats>,
}

/// The deterministic fold core: message intake, watermark tracking, window
/// closing. Pure (no MQTT, no clocks) — unit-tested in isolation.
#[derive(Debug)]
pub struct Aggregation {
    ideal_cycle_ms: u32,
    minute_ms: u32,
    expect_nodes: Vec<String>,
    inputs: WindowInputs,
    /// Last seen `t_ms` per source (stream order guarantees monotonicity).
    last_t: [Option<u32>; 3],
    /// End markers seen per source, with their stream-end times.
    end_t: [Option<u32>; 3],
    /// Minute boundaries already closed (the next window starts here).
    closed_to: u32,
    run_id: String,
    parse_errors: usize,
}

/// One output row of a drain: `(scope, stats)`.
pub struct WindowRow {
    pub scope: &'static str,
    pub stats: WindowStats,
}

impl Aggregation {
    pub fn new(config: &Config) -> Self {
        Self {
            ideal_cycle_ms: config.ideal_cycle_ms,
            minute_ms: config.minute_ms.max(1),
            expect_nodes: config.expect_nodes.clone(),
            inputs: WindowInputs::default(),
            last_t: [None; 3],
            end_t: [None; 3],
            closed_to: 0,
            run_id: String::new(),
            parse_errors: 0,
        }
    }

    /// Parse errors so far (the error-isolation counter).
    pub fn parse_errors(&self) -> usize {
        self.parse_errors
    }

    /// The run id seen last (payload echo).
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Feeds one message (topic relative to the prefix, e.g. `a/status`).
    /// A payload that does not parse is counted and skipped — the run
    /// continues.
    pub fn on_message(&mut self, topic: &str, payload: &str) {
        if let Some(run_id) = str_field(payload, "run_id") {
            self.run_id = run_id.to_string();
        }
        let t_ms = u32_field(payload, "t_ms");
        match topic {
            topics::A_STATUS => {
                let (Some(t_ms), Some(state)) = (t_ms, str_field(payload, "state")) else {
                    self.parse_errors += 1;
                    return;
                };
                self.inputs.statuses.push((t_ms, state == "run"));
                self.last_t[SOURCES.iter().position(|s| *s == "a").unwrap()] = Some(t_ms);
            }
            topics::P_COUNT => {
                let (Some(t_ms), Some(count)) = (t_ms, u32_field(payload, "count")) else {
                    self.parse_errors += 1;
                    return;
                };
                self.inputs.counts.push((t_ms, count));
                self.last_t[SOURCES.iter().position(|s| *s == "p").unwrap()] = Some(t_ms);
            }
            topics::Q_VERDICT => {
                let (Some(t_ms), Some(verdict)) = (t_ms, str_field(payload, "verdict")) else {
                    self.parse_errors += 1;
                    return;
                };
                self.inputs.verdicts.push((t_ms, verdict == "good"));
                self.last_t[SOURCES.iter().position(|s| *s == "q").unwrap()] = Some(t_ms);
            }
            _ => {
                if let Some(node) = topic.strip_suffix("/end") {
                    if SOURCES.contains(&node) {
                        match t_ms {
                            Some(t_ms) => {
                                let at = SOURCES.iter().position(|s| s == &node).unwrap();
                                self.end_t[at] = Some(t_ms);
                            }
                            None => self.parse_errors += 1,
                        }
                    }
                }
            }
        }
    }

    /// The watermark: `min(last t_ms of every expected, not-yet-ended
    /// source)`; a source that never spoke holds it at 0.
    pub fn watermark(&self) -> u32 {
        self.last_t
            .iter()
            .enumerate()
            .filter(|(at, _)| self.end_t[*at].is_none() && self.expects(SOURCES[*at]))
            .map(|(_, last)| last.unwrap_or(0))
            .min()
            .unwrap_or(0)
    }

    /// Whether every expected node has published its end marker.
    pub fn all_ended(&self) -> bool {
        SOURCES
            .iter()
            .enumerate()
            .filter(|(at, _)| self.expects(SOURCES[*at]))
            .all(|(at, _)| self.end_t[at].is_some())
    }

    /// The stream end for the final flush: the max of the end markers and
    /// every event time seen (defensive — end markers are published after
    /// the stream, so the max is always an end marker in practice).
    pub fn final_t(&self) -> u32 {
        let ends = self.end_t.iter().flatten().copied().max().unwrap_or(0);
        let events = self
            .inputs
            .statuses
            .last()
            .map(|(t, _)| *t)
            .unwrap_or(0)
            .max(self.inputs.counts.last().map(|(t, _)| *t).unwrap_or(0))
            .max(self.inputs.verdicts.last().map(|(t, _)| *t).unwrap_or(0));
        ends.max(events)
    }

    /// Closes every full minute window with an end <= `upto`; returns the
    /// rows (minute + cumulative shift per boundary).
    pub fn drain(&mut self, upto: u32) -> Vec<WindowRow> {
        let mut rows = Vec::new();
        while self.closed_to.saturating_add(self.minute_ms) <= upto {
            let from = self.closed_to;
            let to = from + self.minute_ms;
            rows.push(WindowRow {
                scope: "minute",
                stats: compute(&self.inputs, from, to, self.ideal_cycle_ms),
            });
            rows.push(WindowRow {
                scope: "shift",
                stats: compute(&self.inputs, 0, to, self.ideal_cycle_ms),
            });
            self.closed_to = to;
        }
        rows
    }

    /// Flushes the final trailing (possibly partial) window and the final
    /// shift row; idempotent by construction of `closed_to`.
    pub fn flush_final(&mut self) -> Vec<WindowRow> {
        let final_t = self.final_t();
        let mut rows = Vec::new();
        if final_t > self.closed_to {
            rows.push(WindowRow {
                scope: "minute",
                stats: compute(&self.inputs, self.closed_to, final_t, self.ideal_cycle_ms),
            });
            self.closed_to = final_t;
        }
        rows.push(WindowRow {
            scope: "shift",
            stats: compute(&self.inputs, 0, final_t, self.ideal_cycle_ms),
        });
        rows
    }

    /// A live cumulative snapshot at the current watermark (MQTT only —
    /// never logged to the CSV, so run-to-run timing jitter cannot leak
    /// into the determinism artifacts).
    pub fn shift_snapshot(&self) -> WindowStats {
        compute(
            &self.inputs,
            0,
            self.watermark().max(1),
            self.ideal_cycle_ms,
        )
    }

    fn expects(&self, node: &str) -> bool {
        self.expect_nodes.iter().any(|expected| expected == node)
    }
}

/// The windows CSV log: `scope,run_id,t_from_ms,t_to_ms,planned_ms,run_ms,
/// parts,good,total,a,p,q,oee` — one row per published window close (the
/// experiment's raw material).
pub struct WindowsCsv<W: Write> {
    writer: csv::Writer<W>,
    rows: usize,
}

impl WindowsCsv<std::fs::File> {
    pub fn create(path: &std::path::Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        Self::new(std::fs::File::create(path)?)
    }
}

impl<W: Write> WindowsCsv<W> {
    pub fn new(out: W) -> std::io::Result<Self> {
        let mut writer = csv::Writer::from_writer(out);
        writer.write_record([
            "scope",
            "run_id",
            "t_from_ms",
            "t_to_ms",
            "planned_ms",
            "run_ms",
            "parts",
            "good",
            "total",
            "a",
            "p",
            "q",
            "oee",
        ])?;
        Ok(Self { writer, rows: 0 })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    /// One row per window close (best-effort, like the node CSV logs).
    pub fn write_row(&mut self, scope: &str, run_id: &str, stats: &WindowStats) {
        let row = [
            scope.to_string(),
            run_id.to_string(),
            stats.t_from_ms.to_string(),
            stats.t_to_ms.to_string(),
            stats.planned_ms.to_string(),
            stats.run_ms.to_string(),
            stats.parts.to_string(),
            stats.good.to_string(),
            stats.total.to_string(),
            format!("{:.4}", stats.availability),
            format!("{:.4}", stats.performance),
            format!("{:.4}", stats.quality),
            format!("{:.4}", stats.oee),
        ];
        if self.writer.write_record(row).is_ok() {
            self.rows += 1;
        }
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}

/// How long an idle read waits before the keepalive check kicks in.
const READ_TIMEOUT: Duration = Duration::from_secs(5);
/// Keepalive: 60 s negotiated, ping at half of that.
const PING_AFTER: Duration = Duration::from_secs(30);

/// Runs the aggregator loop until every expected node has ended: connect,
/// subscribe, fold, publish, log. Returns the run summary (with the final
/// shift row — the measured OEE of the bench run). The error is boxed and
/// `Send` — the experiment runs the loop on a thread.
pub fn run(config: &Config) -> Result<RunSummary, Box<dyn std::error::Error + Send + Sync>> {
    let mut client = Client::connect(&config.broker_addr, "oee-aggregator", 60)?;
    let prefix = &config.topic_prefix;
    client.subscribe(&format!("{prefix}/{}", topics::A_STATUS))?;
    client.subscribe(&format!("{prefix}/{}", topics::P_COUNT))?;
    client.subscribe(&format!("{prefix}/{}", topics::Q_VERDICT))?;
    client.subscribe(&format!("{prefix}/{}", topics::END_FILTER))?;
    if let Some(ready) = &config.ready {
        let _ = ready.send(());
    }

    let mut csv = config
        .csv_path
        .as_deref()
        .map(WindowsCsv::create)
        .transpose()?;
    let mut fold = Aggregation::new(config);
    let oee_topic = format!("{prefix}/{}", topics::OEE);
    let mut summary = RunSummary::default();
    let mut last_ping = Instant::now();

    loop {
        if fold.all_ended() {
            let rows = fold.flush_final();
            summary.final_shift = rows
                .iter()
                .find(|row| row.scope == "shift")
                .map(|row| row.stats);
            for row in rows {
                client.publish(
                    &oee_topic,
                    &oee_payload(row.scope, fold.run_id(), &row.stats),
                )?;
                summary.publishes += 1;
                summary.windows += usize::from(row.scope == "minute");
                if let Some(csv) = csv.as_mut() {
                    csv.write_row(row.scope, fold.run_id(), &row.stats);
                }
            }
            if let Some(csv) = csv.as_mut() {
                csv.flush()?;
            }
            summary.parse_errors = fold.parse_errors();
            break;
        }
        match client.next_message(READ_TIMEOUT)? {
            Some(message) => {
                let Some(suffix) = message.topic.strip_prefix(&format!("{prefix}/")) else {
                    continue;
                };
                fold.on_message(suffix, &message.payload);
                summary.messages += 1;
                // Advance windows past the watermark, then publish a live
                // shift snapshot (the dashboard's "numbers that live").
                let rows = fold.drain(fold.watermark());
                for row in rows {
                    client.publish(
                        &oee_topic,
                        &oee_payload(row.scope, fold.run_id(), &row.stats),
                    )?;
                    summary.publishes += 1;
                    summary.windows += usize::from(row.scope == "minute");
                    if let Some(csv) = csv.as_mut() {
                        csv.write_row(row.scope, fold.run_id(), &row.stats);
                        // A live bench is Ctrl-C'd mid-run: keep the CSV
                        // usable without a graceful exit.
                        let _ = csv.flush();
                    }
                }
                let snapshot = fold.shift_snapshot();
                client.publish(&oee_topic, &oee_payload("shift", fold.run_id(), &snapshot))?;
                summary.publishes += 1;
            }
            None => {
                if last_ping.elapsed() >= PING_AFTER {
                    client.send_ping()?;
                    last_ping = Instant::now();
                }
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config {
            ideal_cycle_ms: 400,
            minute_ms: 60_000,
            expect_nodes: vec!["a".into(), "p".into(), "q".into()],
            ..Config::default()
        }
    }

    /// A miniature coherent run: statuses, counts, verdicts interleaved the
    /// way three threads would deliver them, then the end markers.
    #[test]
    fn fold_closes_windows_and_flushes_the_final_shift() {
        let mut fold = Aggregation::new(&config());
        // Before any data: watermark 0, nothing closed.
        assert_eq!(fold.watermark(), 0);
        assert!(!fold.all_ended());

        // A 90 ms run (a) with 2 parts (p) and 2 verdicts (q) — machine
        // time 0..90 s; a minute window [0, 60 s) closes once every source
        // passes 60 s.
        fold.on_message("a/status", r#"{"state":"run","t_ms":1000,"run_id":"r1"}"#);
        fold.on_message("p/count", r#"{"count":1,"t_ms":10000,"run_id":"r1"}"#);
        fold.on_message(
            "q/verdict",
            r#"{"verdict":"good","t_ms":10000,"run_id":"r1"}"#,
        );
        fold.on_message("p/count", r#"{"count":2,"t_ms":30000,"run_id":"r1"}"#);
        fold.on_message(
            "q/verdict",
            r#"{"verdict":"cracked","t_ms":30000,"run_id":"r1"}"#,
        );
        // Watermark still 1000 (a is behind): nothing closes.
        assert_eq!(fold.watermark(), 1000);
        assert!(fold.drain(fold.watermark()).is_empty());

        fold.on_message("a/status", r#"{"state":"idle","t_ms":70000,"run_id":"r1"}"#);
        // Now min(70_000, 30_000, 30_000) = 30_000 — still no close.
        assert_eq!(fold.watermark(), 30_000);
        fold.on_message("p/count", r#"{"count":3,"t_ms":65000,"run_id":"r1"}"#);
        fold.on_message(
            "q/verdict",
            r#"{"verdict":"good","t_ms":65000,"run_id":"r1"}"#,
        );
        // Watermark 65_000: the minute window [0, 60 s) closes with
        // run 59 s, parts 2, verdicts 2 (1 good).
        let rows = fold.drain(fold.watermark());
        assert_eq!(rows.len(), 2, "minute + shift per boundary");
        let minute = rows.iter().find(|r| r.scope == "minute").unwrap();
        assert_eq!(minute.stats.t_to_ms, 60_000);
        assert_eq!(minute.stats.run_ms, 59_000);
        assert_eq!(minute.stats.parts, 2);
        assert_eq!(minute.stats.total, 2);
        assert_eq!(minute.stats.good, 1);
        let shift = rows.iter().find(|r| r.scope == "shift").unwrap();
        assert_eq!(shift.stats.t_to_ms, 60_000);

        // End markers: the flush closes [60 s, 90 s) and the final shift.
        fold.on_message("a/end", r#"{"t_ms":90000,"run_id":"r1"}"#);
        fold.on_message("p/end", r#"{"t_ms":80000,"run_id":"r1"}"#);
        assert!(!fold.all_ended(), "q has not ended yet");
        fold.on_message("q/end", r#"{"t_ms":85000,"run_id":"r1"}"#);
        assert!(fold.all_ended());
        assert_eq!(fold.final_t(), 90_000);
        let rows = fold.flush_final();
        assert_eq!(rows.len(), 2, "partial minute + final shift");
        let partial = rows.iter().find(|r| r.scope == "minute").unwrap();
        assert_eq!(partial.stats.t_from_ms, 60_000);
        assert_eq!(partial.stats.t_to_ms, 90_000);
        assert_eq!(partial.stats.parts, 1, "the part at 65 s is in the partial");
        let final_shift = rows.iter().find(|r| r.scope == "shift").unwrap();
        assert_eq!(final_shift.stats.t_to_ms, 90_000);
        assert_eq!(final_shift.stats.parts, 3);
        assert_eq!(fold.run_id(), "r1");
        assert_eq!(fold.parse_errors(), 0);
        // Idempotency: a second flush re-emits only the final shift row.
        assert_eq!(fold.flush_final().len(), 1);
    }

    #[test]
    fn watermark_ignores_unexpected_sources() {
        // An aggregator expecting only a and p must not wait on q's end.
        let config = Config {
            expect_nodes: vec!["a".into(), "p".into()],
            ..config()
        };
        let mut fold = Aggregation::new(&config);
        fold.on_message("q/verdict", r#"{"verdict":"good","t_ms":50000}"#);
        assert_eq!(
            fold.watermark(),
            0,
            "q does not hold the watermark back when not expected"
        );
        fold.on_message("a/status", r#"{"state":"run","t_ms":10000}"#);
        fold.on_message("p/count", r#"{"count":1,"t_ms":10000}"#);
        fold.on_message("a/end", r#"{"t_ms":60000}"#);
        fold.on_message("p/end", r#"{"t_ms":60000}"#);
        assert!(fold.all_ended());
        assert_eq!(fold.final_t(), 60_000);
    }

    #[test]
    fn corrupt_payloads_are_counted_and_skipped() {
        let mut fold = Aggregation::new(&config());
        fold.on_message("a/status", "garbage");
        fold.on_message("p/count", r#"{"count":1}"#); // no t_ms
        fold.on_message("q/verdict", r#"{"t_ms":5}"#); // no verdict
        fold.on_message("a/status", r#"{"state":"run","t_ms":1000}"#);
        assert_eq!(fold.parse_errors(), 3);
        assert_eq!(fold.inputs.statuses.len(), 1, "the good row still lands");
        assert_eq!(fold.watermark(), 0, "p/q never spoke a valid time");
    }

    #[test]
    fn csv_log_writes_the_pinned_schema() {
        let mut log = WindowsCsv::new(Vec::new()).unwrap();
        let stats = crate::windows::compute(&WindowInputs::default(), 0, 60_000, 400);
        log.write_row("shift", "run1", &stats);
        log.write_row("minute", "run1", &stats);
        assert_eq!(log.rows(), 2);
        let bytes = log.writer.into_inner().unwrap();
        let text = String::from_utf8(bytes).unwrap();
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap(),
            "scope,run_id,t_from_ms,t_to_ms,planned_ms,run_ms,parts,good,total,a,p,q,oee"
        );
        assert!(lines
            .next()
            .unwrap()
            .starts_with("shift,run1,0,60000,60000,0,0,0,0,0.0000"));
    }
}
