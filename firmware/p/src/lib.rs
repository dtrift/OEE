//! Node P firmware logic (Performance) — the `no_std` core.
//!
//! The TCRT5000 barrier twin of the host `nodes::p::EdgeCounter`: a rising
//! edge counts a part, edges inside the anti-double window (the hardware
//! track uses the `firmware/README` 50 ms contact-bounce figure — the host
//! 100 ms figure is for the simulator's double pulses; the bench contact
//! bounce is shorter) are swallowed. Pure logic, host-tested.

#![no_std]

use core::fmt::Write as _;

use fmt_util::Cursor;

/// The contact-bounce window, ms (firmware/README; the host node's
/// `ANTI_DOUBLE_MS = 100` targets the simulator's double pulses).
pub const DEBOUNCE_MS: u32 = 50;

/// The rising-edge counter with an anti-double window.
///
/// Mirrors the host `nodes::p::EdgeCounter` mechanics: only a rising edge
/// (low -> high) is a part candidate, and the anti-double window is
/// anchored to the last *counted* part (not extended by swallowed edges —
/// an endless glitch storm must not freeze the counter). `DEBOUNCE_MS`
/// stays a hardware-specific figure (50 ms vs the host's simulator-facing
/// 100 ms); the mechanics are identical, pinned by the shared-trace test.
pub struct EdgeCounter {
    debounce_ms: u32,
    last_level: Option<bool>,
    last_counted: Option<u32>,
    count: u32,
}

impl EdgeCounter {
    pub const fn new(debounce_ms: u32) -> Self {
        Self {
            debounce_ms,
            last_level: None,
            last_counted: None,
            count: 0,
        }
    }

    /// Feeds one barrier level at machine time `t_ms`; returns the new
    /// count when a part was counted (`None` — no part in this sample).
    /// The first sample establishes the level baseline: a barrier already
    /// blocked at boot is not a rising edge (no phantom part).
    pub fn observe(&mut self, level: bool, t_ms: u32) -> Option<u32> {
        let rose = self.last_level == Some(false) && level;
        self.last_level = Some(level);
        if !rose {
            return None;
        }
        if let Some(last) = self.last_counted {
            // Saturating (as on the host): a hand-edited stream with a
            // backwards `t_ms` reads as "inside the window" — merge, do
            // not panic or miscount.
            if t_ms.saturating_sub(last) < self.debounce_ms {
                return None;
            }
        }
        self.last_counted = Some(t_ms);
        self.count += 1;
        Some(self.count)
    }

    /// Parts counted so far.
    pub const fn count(&self) -> u32 {
        self.count
    }
}

/// Formats a count line for the UART bridge: `p,<run_id>,<t_ms>,<count>`.
pub fn format_count(out: &mut [u8], run_id: &str, t_ms: u32, count: u32) -> Option<usize> {
    let mut cursor = Cursor::new(out);
    writeln!(cursor, "p,{run_id},{t_ms},{count}").ok()?;
    Some(cursor.pos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_inside_the_window_is_swallowed() {
        let mut counter = EdgeCounter::new(50);
        // The low baseline first: a rising edge needs a preceding low.
        assert_eq!(counter.observe(false, 0), None);
        // A part at t=100.
        assert_eq!(counter.observe(true, 100), Some(1));
        // Contact bounce: quick re-triggers inside 50 ms of the count.
        assert_eq!(counter.observe(false, 110), None);
        assert_eq!(counter.observe(true, 115), None);
        assert_eq!(counter.observe(false, 130), None);
        assert_eq!(counter.observe(true, 135), None);
        // The window is anchored to the counted part (t=100): a real part
        // 110 ms after it counts.
        assert_eq!(counter.observe(false, 209), None);
        assert_eq!(counter.observe(true, 210), Some(2));
        assert_eq!(counter.count(), 2);
    }

    #[test]
    fn low_levels_never_count() {
        let mut counter = EdgeCounter::new(50);
        assert_eq!(counter.observe(false, 0), None);
        assert_eq!(counter.observe(false, 10_000), None);
        assert_eq!(counter.observe(true, 10_050), Some(1));
    }

    #[test]
    fn held_high_counts_exactly_once() {
        let mut counter = EdgeCounter::new(50);
        assert_eq!(counter.observe(false, 0), None);
        assert_eq!(counter.observe(true, 100), Some(1));
        // 400 ms of a held level — a level, not an edge: exactly one part.
        for t in (101..500).step_by(7) {
            assert_eq!(counter.observe(true, t), None);
        }
        assert_eq!(counter.count(), 1);
    }

    #[test]
    fn high_at_boot_is_not_a_part_until_a_low_baseline() {
        // The barrier blocked at power-on: the first high without a
        // preceding low is not a rising edge (no phantom part).
        let mut counter = EdgeCounter::new(50);
        assert_eq!(counter.observe(true, 0), None);
        assert_eq!(counter.observe(true, 10_000), None);
        assert_eq!(counter.observe(false, 10_050), None);
        assert_eq!(counter.observe(true, 10_100), Some(1));
    }

    #[test]
    fn anchored_window_recovers_after_a_bounce_storm() {
        // Glitches 30 ms apart: the first merges, but the window does not
        // extend — a glitch past 50 ms from the last COUNT is a new part
        // (the host semantics; an extending window would freeze here).
        let mut counter = EdgeCounter::new(50);
        assert_eq!(counter.observe(false, 0), None);
        assert_eq!(counter.observe(true, 1000), Some(1));
        assert_eq!(counter.observe(false, 1029), None);
        assert_eq!(counter.observe(true, 1030), None);
        assert_eq!(counter.observe(false, 1059), None);
        assert_eq!(counter.observe(true, 1060), Some(2));
    }

    /// The pinned trace shared with the host `nodes::p::EdgeCounter`
    /// (`mirror_trace_matches_the_firmware_counter` there): both counters
    /// with the same 100 ms window must agree — the "mirroring" the docs
    /// claim, as a fact.
    #[test]
    fn mirror_trace_matches_the_host_counter() {
        let mut counter = EdgeCounter::new(100);
        assert_eq!(counter.observe(false, 0), None);
        assert_eq!(counter.observe(true, 400), Some(1));
        assert_eq!(counter.observe(false, 430), None);
        assert_eq!(counter.observe(true, 470), None);
        assert_eq!(counter.observe(false, 500), None);
        assert_eq!(counter.observe(true, 800), Some(2));
        assert_eq!(counter.observe(false, 830), None);
        assert_eq!(counter.observe(true, 1200), Some(3));
        assert_eq!(counter.observe(true, 1210), None);
        assert_eq!(counter.observe(false, 1300), None);
        assert_eq!(counter.observe(true, 1400), Some(4));
    }

    #[test]
    fn count_line_format() {
        let mut buf = [0u8; 48];
        let n = format_count(&mut buf, "bench-p", 777, 131).unwrap();
        assert_eq!(&buf[..n], b"p,bench-p,777,131\n");
    }
}
