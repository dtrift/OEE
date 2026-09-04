//! Node P: IR barrier -> edge detector -> part counting (week 5, D1).
//!
//! Event-driven by contract (`WindowSpec(P) = None`,
//! `features_cli::window_spec`): no window assembly, no hysteresis — parts
//! are independent point events. The pipeline is a two-stage guard against
//! the belt's edge cases (see `line_simulator::belt`):
//!
//! 1. **Edge detection**: only a rising edge (clear -> blocked) is a part
//!    candidate; falling edges and repeated levels are ignored.
//! 2. **Anti-double-count**: a rising edge within `ANTI_DOUBLE_MS` of the
//!    last *counted* part is a re-trigger of the same part (the belt's
//!    `double`), not a new one. The window (100 ms) sits between the belt's
//!    double-pulse span (defaults: second rise at 70 ms) and the closest
//!    real parts (defaults: >= 320 ms apart).
//!
//! Truth contract: on the belt stream the count equals the belt meta rows
//! exactly — doubles merge into one, skips produce nothing (checked by the
//! integration tests and the week-5 experiment).

use crate::sim_source::IrSource;
use crate::status::{StatusRow, StatusSink};

/// Anti-double-count window, ms: two rising edges closer than this are one
/// part. Must stay above the belt's `pulse_ms + double_gap_ms` and below
/// the shortest real part interval (`period_ms * (1 - jitter)`).
pub const ANTI_DOUBLE_MS: u32 = 100;

/// Node P purpose.
pub fn describe() -> &'static str {
    "node P: ir barrier -> edge detect -> count"
}

/// The edge detector + anti-double-counter: feed it barrier levels with
/// timestamps; it confirms parts.
///
/// Pure and deterministic (no clocks, no RNG) — unit-testable in isolation
/// from any source.
#[derive(Debug)]
pub struct EdgeCounter {
    anti_double_ms: u32,
    last_level: Option<u8>,
    last_counted: Option<u32>,
    /// Rising edges seen (before merging).
    rising_edges: usize,
    /// Rising edges swallowed by the anti-double window.
    merged: usize,
    parts: u32,
}

impl EdgeCounter {
    pub fn new(anti_double_ms: u32) -> Self {
        Self {
            anti_double_ms,
            last_level: None,
            last_counted: None,
            rising_edges: 0,
            merged: 0,
            parts: 0,
        }
    }

    /// Parts counted so far.
    pub fn parts(&self) -> u32 {
        self.parts
    }

    /// Rising edges swallowed by the anti-double window so far.
    pub fn merged(&self) -> usize {
        self.merged
    }

    /// Rising edges seen so far (diagnostics: edges - merged - 1st edges
    /// should equal parts).
    pub fn rising_edges(&self) -> usize {
        self.rising_edges
    }

    /// Feeds one barrier level; returns the new cumulative count when a
    /// part is confirmed.
    pub fn on_level(&mut self, t_ms: u32, level: u8) -> Option<u32> {
        let rose = self.last_level == Some(0) && level == 1;
        self.last_level = Some(level);
        if !rose {
            return None;
        }
        self.rising_edges += 1;
        let within_window = self
            .last_counted
            .is_some_and(|last| t_ms.saturating_sub(last) < self.anti_double_ms);
        if within_window {
            self.merged += 1;
            return None;
        }
        self.parts += 1;
        self.last_counted = Some(t_ms);
        Some(self.parts)
    }
}

/// The offline node P run summary (the gate D1 numbers).
#[derive(Debug, Default, PartialEq)]
pub struct RunSummary {
    /// Rising edges seen.
    pub rising_edges: usize,
    /// Parts counted (count = truth on the belt stream).
    pub parts: u32,
    /// Rising edges merged away by the anti-double window.
    pub merged: usize,
    /// Malformed source rows skipped (error isolation).
    pub bad_rows: usize,
}

/// Runs node P over a belt-events source: edges -> anti-double -> the sink
/// (one status row per counted part; `state` carries the cumulative count).
/// The run ends only on `Exhausted`; bad rows isolate to skipped events.
pub fn run_p<R: std::io::Read>(
    source: &mut IrSource<R>,
    run_id: &str,
    sink: &mut dyn StatusSink,
) -> RunSummary {
    let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
    while let Ok((t_ms, level)) = source.next_event() {
        if let Some(count) = counter.on_level(t_ms, level) {
            sink.on_status(&StatusRow {
                node: "p",
                run_id: run_id.to_string(),
                t_ms,
                state: count.to_string(),
            });
        }
    }
    RunSummary {
        rising_edges: counter.rising_edges(),
        parts: counter.parts(),
        merged: counter.merged(),
        bad_rows: source.bad_rows(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rising_edges_count_falling_ignored() {
        let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
        assert_eq!(counter.on_level(0, 0), None, "the baseline is not a part");
        assert_eq!(counter.on_level(400, 1), Some(1));
        assert_eq!(counter.on_level(430, 0), None, "the fall is not a part");
        assert_eq!(counter.on_level(800, 1), Some(2));
        assert_eq!(counter.parts(), 2);
        assert_eq!(counter.rising_edges(), 2);
    }

    #[test]
    fn a_double_is_one_part() {
        // Two pulses 70 ms apart (the belt's double shape): one part.
        let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
        counter.on_level(0, 0);
        assert_eq!(counter.on_level(400, 1), Some(1));
        assert_eq!(counter.on_level(430, 0), None);
        assert_eq!(counter.on_level(470, 1), None, "the re-trigger merges");
        assert_eq!(counter.on_level(500, 0), None);
        assert_eq!(counter.on_level(800, 1), Some(2), "the next part counts");
        assert_eq!(counter.merged(), 1);
        assert_eq!(counter.parts(), 2);
    }

    #[test]
    fn close_real_parts_are_not_merged() {
        // Two rises 150 ms apart (> the 100 ms window) with a fall between:
        // two parts — the window must only swallow genuine re-triggers.
        let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
        counter.on_level(0, 0);
        assert_eq!(counter.on_level(400, 1), Some(1));
        assert_eq!(counter.on_level(430, 0), None);
        assert_eq!(counter.on_level(550, 1), Some(2));
    }

    #[test]
    fn out_of_order_timestamps_do_not_underflow() {
        let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
        counter.on_level(0, 0);
        assert_eq!(counter.on_level(400, 1), Some(1));
        counter.on_level(430, 0);
        // A late row (t jumped back): saturating arithmetic reads the
        // difference as 0 — inside the window — so it merges instead of
        // panicking or miscounting (the belt stream is sorted; this is a
        // defensive path for a hand-edited CSV).
        assert_eq!(counter.on_level(300, 1), None);
        assert_eq!(counter.parts(), 1);
    }

    #[test]
    fn repeated_blocked_level_is_not_an_edge() {
        let mut counter = EdgeCounter::new(ANTI_DOUBLE_MS);
        counter.on_level(0, 0);
        assert_eq!(counter.on_level(400, 1), Some(1));
        assert_eq!(counter.on_level(405, 1), None, "still blocked: no edge");
    }
}
