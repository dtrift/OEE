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
