//! Node P firmware logic (Performance) — the `no_std` core.
//!
//! The TCRT5000 barrier twin of the host `nodes::p::EdgeCounter`: a rising
//! edge counts a part, edges inside the anti-double window (the hardware
//! track uses the `firmware/README` 50 ms contact-bounce figure — the host
//! 100 ms figure is for the simulator's double pulses; the bench contact
//! bounce is shorter) are swallowed. Pure logic, host-tested.

#![no_std]

use core::fmt::Write as _;

/// The contact-bounce window, ms (firmware/README; the host node's
/// `ANTI_DOUBLE_MS = 100` targets the simulator's double pulses).
pub const DEBOUNCE_MS: u32 = 50;

/// The rising-edge counter with an anti-double window.
pub struct EdgeCounter {
    debounce_ms: u32,
    last_edge_ms: Option<u32>,
    count: u32,
}

impl EdgeCounter {
    pub const fn new(debounce_ms: u32) -> Self {
        Self {
            debounce_ms,
            last_edge_ms: None,
            count: 0,
        }
    }

    /// Feeds one barrier level at machine time `t_ms`; returns the new
    /// count when a part was counted (`None` — no part in this sample).
    pub fn observe(&mut self, level: bool, t_ms: u32) -> Option<u32> {
        if !level {
            return None; // only rising edges count
        }
        if let Some(last) = self.last_edge_ms {
            // Wrap-safe interval: a bench session is hours, u32 ms covers
            // ~49 days; the wrap case simply treats the edge as fresh.
            if t_ms.wrapping_sub(last) <= self.debounce_ms {
                self.last_edge_ms = Some(t_ms); // extend the window
                return None;
            }
        }
        self.last_edge_ms = Some(t_ms);
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

/// A minimal fixed-buffer write cursor (the `firmware-a` twin).
struct Cursor<'a> {
    buf: &'a mut [u8],
    written: usize,
}

impl<'a> Cursor<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, written: 0 }
    }

    fn pos(&self) -> usize {
        self.written
    }
}

impl core::fmt::Write for Cursor<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        if bytes.len() > self.buf.len() {
            return Err(core::fmt::Error);
        }
        self.buf[..bytes.len()].copy_from_slice(bytes);
        self.buf = &mut core::mem::take(&mut self.buf)[bytes.len()..];
        self.written += bytes.len();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounce_inside_the_window_is_swallowed() {
        let mut counter = EdgeCounter::new(50);
        // A part at t=100.
        assert_eq!(counter.observe(true, 100), Some(1));
        // Contact bounce: 3 quick re-triggers inside 50 ms.
        assert_eq!(counter.observe(true, 110), None);
        assert_eq!(counter.observe(true, 130), None);
        assert_eq!(counter.observe(true, 149), None);
        // The bounce window slid with each trigger: a real part 60 ms
        // after the last bounce still counts.
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
    fn count_line_format() {
        let mut buf = [0u8; 48];
        let n = format_count(&mut buf, "bench-p", 777, 131).unwrap();
        assert_eq!(&buf[..n], b"p,bench-p,777,131\n");
    }
}
