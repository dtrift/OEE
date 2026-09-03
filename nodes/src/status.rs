//! Node status machinery (week 4, D1/D2): window assembly, the anti-flap
//! hysteresis, and the status sinks (offline CSV + in-memory collector).
//!
//! The offline CSV follows the `capture` schema family
//! (`features_cli::capture`): `node,run_id,t_ms,state`, one row per status
//! **change** (including the initial state) — the MQTT publisher emits on
//! the same change points.

use std::io::Write;

/// One emitted status/verdict change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusRow {
    /// Node letter: `a` or `q` (`features_cli::NodeKind::as_str`).
    pub node: &'static str,
    pub run_id: String,
    /// Time of the window that confirmed the status, ms.
    pub t_ms: u32,
    /// Status/verdict name (`MachineState::as_str` / `Verdict::as_str`).
    pub state: String,
}

/// Where confirmed statuses go (the offline log, MQTT, a test collector).
pub trait StatusSink {
    fn on_status(&mut self, row: &StatusRow);
}

/// Collects statuses in memory (tests, summaries).
#[derive(Default)]
pub struct VecSink(pub Vec<StatusRow>);

impl StatusSink for VecSink {
    fn on_status(&mut self, row: &StatusRow) {
        self.0.push(row.clone());
    }
}

/// Fans statuses out to several sinks (offline CSV + MQTT is the standard
/// pairing: MQTT failing must never lose the offline row).
pub struct MultiSink<A: StatusSink, B: StatusSink>(pub A, pub B);

impl<A: StatusSink, B: StatusSink> StatusSink for MultiSink<A, B> {
    fn on_status(&mut self, row: &StatusRow) {
        self.0.on_status(row);
        self.1.on_status(row);
    }
}

/// `&mut Sink` is a sink — the fan-out slots accept borrows.
impl<S: StatusSink + ?Sized> StatusSink for &mut S {
    fn on_status(&mut self, row: &StatusRow) {
        (**self).on_status(row);
    }
}

/// The offline status CSV: `node,run_id,t_ms,state`.
pub struct CsvStatusLog<W: Write> {
    writer: csv::Writer<W>,
    rows: usize,
}

impl<W: Write> CsvStatusLog<W> {
    pub fn new(out: W) -> std::io::Result<Self> {
        let mut writer = csv::Writer::from_writer(out);
        writer.write_record(["node", "run_id", "t_ms", "state"])?;
        Ok(Self { writer, rows: 0 })
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }

    /// Unwraps the underlying writer (flushed); the error is boxed — the
    /// csv error type is large.
    pub fn into_inner(self) -> Result<W, Box<csv::IntoInnerError<csv::Writer<W>>>> {
        self.writer.into_inner().map_err(Box::new)
    }
}

impl<W: Write> StatusSink for CsvStatusLog<W> {
    fn on_status(&mut self, row: &StatusRow) {
        // A row-write failure must not kill the node (error isolation):
        // the offline log is best-effort after the first failure.
        let written =
            self.writer
                .write_record([row.node, &row.run_id, &row.t_ms.to_string(), &row.state]);
        if written.is_ok() {
            self.rows += 1;
        }
    }
}

/// The outcome of feeding one sample into a [`WindowAccumulator`].
#[derive(Debug, PartialEq)]
pub enum WindowOutcome {
    /// The window is complete and clean.
    Complete(std::vec::Vec<f32>),
    /// The window completed but a bad sample was seen inside it — dropped.
    Dirty,
    /// More samples needed.
    Filling,
}

/// Non-overlapping window assembly (node A: 128 @ 1.6 kHz).
pub struct WindowAccumulator {
    window: std::vec::Vec<f32>,
    dirty: bool,
}

impl WindowAccumulator {
    pub fn new(window_len: usize) -> Self {
        Self {
            window: Vec::with_capacity(window_len),
            dirty: false,
        }
    }

    /// Feeds one sample; a `None` sample (source hiccup) marks the window
    /// dirty without stopping the stream.
    pub fn push(&mut self, sample: Option<f32>, window_len: usize) -> WindowOutcome {
        match sample {
            Some(value) if !value.is_nan() => self.window.push(value),
            _ => self.dirty = true,
        }
        if self.window.len() < window_len {
            return WindowOutcome::Filling;
        }
        let window = std::mem::take(&mut self.window);
        let dirty = core::mem::take(&mut self.dirty);
        if dirty {
            WindowOutcome::Dirty
        } else {
            WindowOutcome::Complete(window)
        }
    }
}

/// Anti-flap hysteresis (D2): a status change is confirmed only after
/// `confirm_after` consecutive windows agree on the new status.
pub struct Hysteresis {
    confirmed: Option<usize>,
    pending: Option<(usize, u32)>,
    confirm_after: u32,
}

impl Hysteresis {
    pub fn new(confirm_after: u32) -> Self {
        Self {
            confirmed: None,
            pending: None,
            confirm_after: confirm_after.max(1),
        }
    }

    /// Feeds one raw prediction; returns the newly confirmed status when it
    /// changed (the initial status counts as a change — it is published).
    pub fn observe(&mut self, prediction: usize) -> Option<usize> {
        match self.pending {
            Some((value, streak)) if value == prediction => {
                self.pending = Some((value, streak + 1));
            }
            _ => {
                self.pending = Some((prediction, 1));
            }
        }
        let (value, streak) = self.pending.expect("just set");
        if streak >= self.confirm_after && self.confirmed != Some(value) {
            self.confirmed = Some(value);
            Some(value)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_complete_non_overlapping() {
        let mut acc = WindowAccumulator::new(4);
        assert_eq!(acc.push(Some(1.0), 4), WindowOutcome::Filling);
        assert_eq!(acc.push(Some(2.0), 4), WindowOutcome::Filling);
        assert_eq!(acc.push(Some(3.0), 4), WindowOutcome::Filling);
        assert_eq!(
            acc.push(Some(4.0), 4),
            WindowOutcome::Complete(vec![1.0, 2.0, 3.0, 4.0])
        );
        // The next window starts fresh.
        assert_eq!(acc.push(Some(5.0), 4), WindowOutcome::Filling);
    }

    #[test]
    fn a_bad_sample_dirties_only_its_window() {
        let mut acc = WindowAccumulator::new(3);
        assert_eq!(acc.push(Some(1.0), 3), WindowOutcome::Filling);
        // A bad sample marks the window dirty without occupying a slot.
        assert_eq!(acc.push(None, 3), WindowOutcome::Filling);
        assert_eq!(acc.push(Some(3.0), 3), WindowOutcome::Filling);
        assert_eq!(acc.push(Some(4.0), 3), WindowOutcome::Dirty);
        // The next window starts clean.
        assert_eq!(
            acc.push(Some(5.0), 3),
            WindowOutcome::Filling,
            "the next window is clean again"
        );
    }

    #[test]
    fn hysteresis_confirms_after_streak() {
        let mut h = Hysteresis::new(2);
        // The initial status is published immediately-ish (streak 1 of 2 —
        // nothing confirmed yet).
        assert_eq!(h.observe(1), None);
        assert_eq!(h.observe(1), Some(1), "the initial status gets published");
        // A single blip does not flip the status.
        assert_eq!(h.observe(2), None);
        assert_eq!(h.observe(1), None);
        // Two consecutive new predictions confirm the change.
        assert_eq!(h.observe(2), None);
        assert_eq!(h.observe(2), Some(2));
        // Repeated confirmations of the same status emit nothing.
        assert_eq!(h.observe(2), None);
    }

    #[test]
    fn csv_log_writes_the_pinned_schema() {
        let mut log = CsvStatusLog::new(Vec::new()).unwrap();
        log.on_status(&StatusRow {
            node: "a",
            run_id: "run1".into(),
            t_ms: 625,
            state: "run".into(),
        });
        let bytes = log.into_inner().unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "node,run_id,t_ms,state\na,run1,625,run\n"
        );
    }
}
