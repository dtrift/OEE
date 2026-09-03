//! Node data sources over the simulator's CSV exports (week 4, D1/D4).
//!
//! - [`SimSource`]: the run CSV (`t_ms,current_a,state`) as an f32 sample
//!   stream — node A's input; exposes the last row's time/state so status
//!   rows carry real timestamps.
//! - [`TapSource`]: the tap dataset (`label,state,x000..x1023`) as a flat
//!   sample stream — node Q's input; each CSV row is exactly one window
//!   (the label column is ignored by the node: prediction must not see the
//!   ground truth; the paired meta file provides timestamps).
//!
//! Error isolation: a malformed row never kills the stream — it is skipped
//! and marks the surrounding window dirty (the node drops that window and
//! keeps running).

use std::io::Read;

use crate::q::WINDOW;
use crate::source::{SensorSource, SourceError};

/// Run-CSV source for node A (`Sample = f32` amperes).
pub struct SimSource<R: Read> {
    reader: csv::Reader<R>,
    /// t_ms of the last served row.
    last_t_ms: u32,
    /// state string of the last served row (ground truth, diagnostics only).
    last_state: String,
    /// A malformed row was seen since the window started — the caller checks
    /// this when the window completes.
    dirty: bool,
    bad_rows: usize,
}

impl<R: Read> SimSource<R> {
    /// Wraps a run CSV (`t_ms,current_a,state`).
    pub fn new(data: R) -> Self {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .from_reader(data);
        // The header contract is pinned; a wrong header is a setup error,
        // not a stream hiccup — surfaced as a dirty first window.
        let header_ok = reader
            .headers()
            .map(|h| h.iter().eq(["t_ms", "current_a", "state"]))
            .unwrap_or(false);
        Self {
            reader,
            last_t_ms: 0,
            last_state: String::new(),
            dirty: !header_ok,
            bad_rows: 0,
        }
    }

    /// Time of the last served sample, ms.
    pub fn last_t_ms(&self) -> u32 {
        self.last_t_ms
    }

    /// Ground-truth state of the last served sample (diagnostics only).
    pub fn last_state(&self) -> &str {
        &self.last_state
    }

    /// Whether a bad row was seen since the last `take_dirty` — the window
    /// assembled from this stretch must be dropped.
    pub fn take_dirty(&mut self) -> bool {
        core::mem::take(&mut self.dirty)
    }

    /// Malformed rows skipped so far (the error-isolation counter).
    pub fn bad_rows(&self) -> usize {
        self.bad_rows
    }
}

impl<R: Read> SensorSource for SimSource<R> {
    type Sample = f32;

    fn next_sample(&mut self) -> Result<f32, SourceError> {
        loop {
            let record = match self.reader.records().next() {
                None => return Err(SourceError::Exhausted),
                Some(Err(_)) => {
                    self.bad_rows += 1;
                    self.dirty = true;
                    continue;
                }
                Some(Ok(record)) => record,
            };
            let parse = (|| -> Option<(u32, f32, String)> {
                let t_ms = record.get(0)?.parse().ok()?;
                let current = record.get(1)?.parse().ok()?;
                let state = record.get(2)?.to_string();
                Some((t_ms, current, state))
            })();
            match parse {
                Some((t_ms, current, state)) => {
                    self.last_t_ms = t_ms;
                    self.last_state = state;
                    if current.is_nan() {
                        // A NaN sample poisons its window but not the stream.
                        self.dirty = true;
                        self.bad_rows += 1;
                        continue;
                    }
                    return Ok(current);
                }
                None => {
                    self.bad_rows += 1;
                    self.dirty = true;
                }
            }
        }
    }
}

/// Tap-dataset source for node Q (`Sample = f32`, relative units).
///
/// Each CSV row (`label,state,x000..x1023`) becomes exactly one
/// [`crate::q::WINDOW`]-sample run; rows are concatenated into one stream.
/// The label/state columns are skipped: the node predicts blind.
pub struct TapSource<R: Read, M: Read> {
    reader: csv::Reader<R>,
    /// Samples of the row being served.
    pending: std::collections::VecDeque<f32>,
    /// t_ms from the paired meta file (`t_ms,verdict`), aligned by row order.
    meta_t_ms: std::vec::Vec<u32>,
    meta_at: usize,
    last_t_ms: u32,
    dirty: bool,
    bad_rows: usize,
    /// The meta reader type is consumed at construction.
    _marker: core::marker::PhantomData<M>,
}

impl<R: Read, M: Read> TapSource<R, M> {
    /// Wraps a taps dataset CSV plus its meta CSV (timestamps by row order).
    pub fn new(data: R, meta: M) -> Self {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .from_reader(data);
        let header_ok = reader
            .headers()
            .map(|h| {
                h.len() == 2 + WINDOW && h.get(0) == Some("label") && h.get(1) == Some("state")
            })
            .unwrap_or(false);

        let mut meta_t_ms = Vec::new();
        let mut meta_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(meta);
        for record in meta_reader.records() {
            let Ok(record) = record else { continue };
            if let Some(t) = record.get(0).and_then(|f| f.parse().ok()) {
                meta_t_ms.push(t);
            }
        }

        Self {
            reader,
            pending: std::collections::VecDeque::new(),
            meta_t_ms,
            meta_at: 0,
            last_t_ms: 0,
            dirty: !header_ok,
            bad_rows: 0,
            _marker: core::marker::PhantomData,
        }
    }

    /// Time of the tap whose samples are being served, ms.
    pub fn last_t_ms(&self) -> u32 {
        self.last_t_ms
    }

    /// Whether a bad row was seen since the last `take_dirty`.
    pub fn take_dirty(&mut self) -> bool {
        core::mem::take(&mut self.dirty)
    }

    /// Malformed rows skipped so far.
    pub fn bad_rows(&self) -> usize {
        self.bad_rows
    }

    /// Loads the next dataset row into `pending`; false when exhausted.
    fn load_row(&mut self) -> bool {
        loop {
            let record = match self.reader.records().next() {
                None => return false,
                Some(Err(_)) => {
                    self.bad_rows += 1;
                    self.dirty = true;
                    continue;
                }
                Some(Ok(record)) => record,
            };
            let parse = (|| -> Option<std::vec::Vec<f32>> {
                if record.len() != 2 + WINDOW {
                    return None;
                }
                let mut samples = Vec::with_capacity(WINDOW);
                for field in record.iter().skip(2) {
                    let value: f32 = field.parse().ok()?;
                    if value.is_nan() {
                        return None;
                    }
                    samples.push(value);
                }
                Some(samples)
            })();
            match parse {
                Some(samples) => {
                    if let Some(&t) = self.meta_t_ms.get(self.meta_at) {
                        self.last_t_ms = t;
                    }
                    self.meta_at += 1;
                    self.pending.extend(samples);
                    return true;
                }
                None => {
                    self.bad_rows += 1;
                    self.dirty = true;
                    // Row counts drift when a dataset row is dropped — the
                    // meta alignment only stays exact while no row is bad;
                    // after a skip the timestamp is the nearest available.
                    if let Some(&t) = self.meta_t_ms.get(self.meta_at) {
                        self.last_t_ms = t;
                    }
                    self.meta_at += 1;
                }
            }
        }
    }
}

impl<R: Read, M: Read> SensorSource for TapSource<R, M> {
    type Sample = f32;

    fn next_sample(&mut self) -> Result<f32, SourceError> {
        while self.pending.is_empty() {
            if !self.load_row() {
                return Err(SourceError::Exhausted);
            }
        }
        self.pending
            .pop_front()
            .ok_or(SourceError::Sensor("pending drained unexpectedly"))
    }
}

/// IR-barrier event source for node P (`Sample = u8`, the barrier level).
///
/// Input: the simulator's belt-events CSV (`t_ms,ir`) — level *changes*
/// only, starting from the idle baseline row. Malformed rows are skipped
/// and counted (error isolation: a bad row must never kill the counter);
/// a level row whose value is not 0/1 counts as bad too.
pub struct IrSource<R: Read> {
    reader: csv::Reader<R>,
    /// t_ms of the last served row.
    last_t_ms: u32,
    /// Barrier level of the last served row (edge detection input).
    last_level: Option<u8>,
    bad_rows: usize,
}

impl<R: Read> IrSource<R> {
    /// Wraps a belt-events CSV (`t_ms,ir`).
    pub fn new(data: R) -> Self {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(false)
            .from_reader(data);
        let header_ok = reader
            .headers()
            .map(|h| h.iter().eq(["t_ms", "ir"]))
            .unwrap_or(false);
        Self {
            reader,
            last_t_ms: 0,
            last_level: None,
            bad_rows: usize::from(!header_ok),
        }
    }

    /// Time of the last served event, ms.
    pub fn last_t_ms(&self) -> u32 {
        self.last_t_ms
    }

    /// Malformed rows skipped so far (the error-isolation counter).
    pub fn bad_rows(&self) -> usize {
        self.bad_rows
    }

    /// Reads the next level change: `(t_ms, level)`. The initial baseline
    /// row (0,0) is served like any other — the edge detector decides.
    pub fn next_event(&mut self) -> Result<(u32, u8), SourceError> {
        loop {
            let record = match self.reader.records().next() {
                None => return Err(SourceError::Exhausted),
                Some(Err(_)) => {
                    self.bad_rows += 1;
                    continue;
                }
                Some(Ok(record)) => record,
            };
            let parse = (|| -> Option<(u32, u8)> {
                let t_ms = record.get(0)?.parse().ok()?;
                let level = record.get(1)?.parse::<u8>().ok()?;
                (level <= 1).then_some((t_ms, level))
            })();
            match parse {
                Some((t_ms, level)) => {
                    self.last_t_ms = t_ms;
                    self.last_level = Some(level);
                    return Ok((t_ms, level));
                }
                None => self.bad_rows += 1,
            }
        }
    }
}

impl<R: Read> SensorSource for IrSource<R> {
    type Sample = u8;

    /// Serves barrier levels (the timestamps ride along via
    /// [`IrSource::last_t_ms`]; node P is event-driven — `WindowSpec(P)` is
    /// `None`, there is nothing to window here).
    fn next_sample(&mut self) -> Result<u8, SourceError> {
        self.next_event().map(|(_, level)| level)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_CSV: &str = "t_ms,current_a,state\n0,0.4,idle\n625,2.0,run\n";

    #[test]
    fn sim_source_streams_samples_with_time() {
        let mut source = SimSource::new(RUN_CSV.as_bytes());
        assert_eq!(source.next_sample(), Ok(0.4));
        assert_eq!(source.last_t_ms(), 0);
        assert_eq!(source.last_state(), "idle");
        assert_eq!(source.next_sample(), Ok(2.0));
        assert_eq!(source.last_t_ms(), 625);
        assert_eq!(source.next_sample(), Err(SourceError::Exhausted));
    }

    #[test]
    fn sim_source_skips_bad_rows_and_marks_dirty() {
        let text = "t_ms,current_a,state\n0,0.4,idle\nbogus row with,extra,cols\n625,NaN,run\n1250,2.0,run\n";
        let mut source = SimSource::new(text.as_bytes());
        assert_eq!(source.next_sample(), Ok(0.4));
        assert!(!source.take_dirty(), "a clean stretch is not dirty");
        // The bogus row and the NaN row are skipped: one good sample remains.
        assert_eq!(source.next_sample(), Ok(2.0));
        assert!(source.take_dirty(), "the stretch with bad rows is dirty");
        assert_eq!(source.bad_rows(), 2);
        assert_eq!(source.next_sample(), Err(SourceError::Exhausted));
    }

    #[test]
    fn sim_source_rejects_wrong_header_by_dirty_flag() {
        let mut source = SimSource::new("a,b,c\n1,2,3\n".as_bytes());
        assert!(source.take_dirty(), "a wrong header marks the window dirty");
    }

    #[test]
    fn ir_source_streams_level_changes() {
        let text = "t_ms,ir\n0,0\n400,1\n430,0\n800,1\n";
        let mut source = IrSource::new(text.as_bytes());
        assert_eq!(source.next_event(), Ok((0, 0)));
        assert_eq!(source.last_t_ms(), 0);
        assert_eq!(source.next_event(), Ok((400, 1)));
        assert_eq!(source.next_event(), Ok((430, 0)));
        assert_eq!(source.next_event(), Ok((800, 1)));
        assert_eq!(source.last_t_ms(), 800);
        assert_eq!(source.next_event(), Err(SourceError::Exhausted));
    }

    #[test]
    fn ir_source_skips_bad_rows() {
        // A malformed row and a level outside 0/1 are skipped, not fatal.
        let text = "t_ms,ir\n0,0\nbogus,row\n400,1\n430,7\n500,0\n";
        let mut source = IrSource::new(text.as_bytes());
        assert_eq!(source.next_event(), Ok((0, 0)));
        assert_eq!(source.next_event(), Ok((400, 1)));
        assert_eq!(source.bad_rows(), 1, "only the malformed row so far");
        // The invalid level (7) is skipped as bad, then the good row flows.
        assert_eq!(source.next_event(), Ok((500, 0)));
        assert_eq!(source.bad_rows(), 2);
    }

    #[test]
    fn ir_source_rejects_wrong_header_by_bad_row_count() {
        let mut source = IrSource::new("a,b\n1,2\n".as_bytes());
        assert_eq!(source.bad_rows(), 1, "a wrong header is a setup error");
        // The single data row has an invalid level (2) — skipped too.
        assert_eq!(source.next_event(), Err(SourceError::Exhausted));
        assert_eq!(source.bad_rows(), 2);
    }

    /// Builds a full-width tap dataset row (2 + WINDOW columns): zeros with
    /// the marker sample last.
    fn tap_row(label: &str, state: &str, sample: f32) -> String {
        let mut fields = vec!["0.0".to_string(); WINDOW - 1];
        fields.push(format!("{sample}"));
        format!("{label},{state},{}", fields.join(","))
    }

    fn tap_header() -> String {
        let xs = (0..WINDOW).map(|i| format!("x{i:03}")).collect::<Vec<_>>();
        format!("label,state,{}", xs.join(","))
    }

    #[test]
    fn tap_source_serves_rows_as_sample_runs() {
        // Two rows x WINDOW samples; meta timestamps align by order.
        let data = format!(
            "{}\n{}\n{}\n",
            tap_header(),
            tap_row("0", "good", 0.1),
            tap_row("1", "cracked", 0.3)
        );
        let meta = "t_ms,verdict\n400,good\n800,cracked\n";
        let mut source = TapSource::new(data.as_bytes(), meta.as_bytes());
        assert!(
            !source.take_dirty(),
            "the header must match the pinned schema"
        );
        // WINDOW-1 zeros then the marker sample, twice.
        for _ in 0..WINDOW - 1 {
            assert_eq!(source.next_sample(), Ok(0.0));
        }
        assert_eq!(source.next_sample(), Ok(0.1));
        assert_eq!(source.last_t_ms(), 400);
        for _ in 0..WINDOW - 1 {
            assert_eq!(source.next_sample(), Ok(0.0));
        }
        assert_eq!(source.next_sample(), Ok(0.3));
        assert_eq!(source.last_t_ms(), 800);
        assert_eq!(source.next_sample(), Err(SourceError::Exhausted));
    }

    #[test]
    fn tap_source_skips_malformed_row() {
        // A row with a non-parseable sample field is skipped whole.
        let mut bad_fields = vec!["0.0".to_string(); WINDOW - 1];
        bad_fields.push("not-a-float".to_string());
        let bad_row = format!("0,good,{}", bad_fields.join(","));
        let data = format!(
            "{}\n{}\n{}\n",
            tap_header(),
            bad_row,
            tap_row("1", "cracked", 0.3)
        );
        let meta = "t_ms,verdict\n400,good\n800,cracked\n";
        let mut source = TapSource::new(data.as_bytes(), meta.as_bytes());
        for _ in 0..WINDOW - 1 {
            assert_eq!(source.next_sample(), Ok(0.0));
        }
        assert_eq!(source.next_sample(), Ok(0.3), "the bad row is skipped");
        assert!(source.take_dirty());
        assert_eq!(source.bad_rows(), 1);
        assert_eq!(source.next_sample(), Err(SourceError::Exhausted));
    }
}
