//! Shared no_std formatting helpers of the firmware workspace (review card
//! 20260909120041: the cursor was copied 3x across the node libs, argmax 2x
//! with a panic path on NaN).

#![no_std]

/// A minimal fixed-buffer write cursor (no alloc; a format overflow is a
/// plain `Err` — the caller passes a generously sized stack buffer).
pub struct Cursor<'a> {
    buf: &'a mut [u8],
    written: usize,
}

impl<'a> Cursor<'a> {
    pub const fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, written: 0 }
    }

    /// Bytes written so far.
    pub const fn pos(&self) -> usize {
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

/// Argmax with a total order — `partial_cmp().unwrap()` panics on NaN,
/// `total_cmp` never does (a positive NaN sorts above every finite value,
/// a negative NaN below). Ties resolve to the LAST maximum, matching
/// `Iterator::max_by` — the host `nodes::a/q` argmax semantics, so the
/// firmware/host classification parity holds on real inputs.
///
/// The slice must be non-empty (index 0 is the fallback otherwise).
pub fn argmax(values: &[f32]) -> usize {
    let mut best = 0;
    for (i, v) in values.iter().enumerate() {
        if v.total_cmp(&values[best]) != core::cmp::Ordering::Less {
            best = i;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write as _;

    #[test]
    fn cursor_writes_and_reports_overflow() {
        let mut buf = [0u8; 8];
        let mut cursor = Cursor::new(&mut buf);
        write!(cursor, "abc").unwrap();
        assert_eq!(cursor.pos(), 3);
        assert!(write!(cursor, "123456").is_err(), "does not fit");
        assert_eq!(cursor.pos(), 3);
    }

    #[test]
    fn argmax_is_nan_safe_and_last_on_ties() {
        assert_eq!(
            argmax(&[1.0, f32::NAN, 2.0]),
            1,
            "+NaN is the total-order max"
        );
        assert_eq!(
            argmax(&[1.0, -f32::NAN, 2.0]),
            2,
            "-NaN is the total-order min"
        );
        assert_eq!(argmax(&[1.0, 1.0, 1.0]), 2, "last maximum, as max_by");
        assert_eq!(argmax(&[-1.0, 5.0, -3.0]), 1);
        assert_eq!(argmax(&[3.0]), 0);
    }
}
