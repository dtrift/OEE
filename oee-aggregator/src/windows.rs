//! Window math of the OEE aggregator (week 5, D2): the per-window
//! components A, P, Q over stored node streams.
//!
//! Inputs are the three node streams as *append-only, per-source
//! time-ordered* arrays (each node publishes in stream order over one TCP
//! connection, so per-source order is guaranteed end-to-end):
//! - A: status changes `(t_ms, is_run)` — a step function;
//! - P: cumulative counts `(t_ms, count)`;
//! - Q: verdicts `(t_ms, is_good)`.
//!
//! All ranges are half-open `[from, to)`: a status/count/verdict exactly at
//! `to` belongs to the next window, one exactly at `from` to this window.
//! Before the first A status the machine counts as not running (the
//! scenario convention: the machine starts idle); the last status extends
//! to the window end.
//!
//! Formulas (plan section 1):
//! - `A = run_ms / planned_ms`;
//! - `P = ideal_cycle_ms * parts / run_ms`, capped at 1.0 (the standard OEE
//!   convention; also guards measured-run undershoot from A noise) — 0.0
//!   when the window had no run time;
//! - `Q = good / total`, and **1.0 when total == 0** (nothing produced,
//!   nothing defective — the cut-line baseline convention);
//! - `OEE = A * P * Q` via [`crate::oee`].

use crate::oee;

/// The stored node streams (append-only, per-source time-ordered).
#[derive(Debug, Default, PartialEq)]
pub struct WindowInputs {
    /// Node A status changes: `(t_ms, is_run)` in stream order.
    pub statuses: Vec<(u32, bool)>,
    /// Node P cumulative counts: `(t_ms, count)` in stream order.
    pub counts: Vec<(u32, u32)>,
    /// Node Q verdicts: `(t_ms, is_good)` in stream order.
    pub verdicts: Vec<(u32, bool)>,
}

impl WindowInputs {
    /// The cumulative part count strictly before `t` (a count exactly at
    /// `t` belongs to the window starting at `t`).
    fn count_before(&self, t: u32) -> u32 {
        self.counts
            .iter()
            .take_while(|(at, _)| *at < t)
            .map(|(_, count)| *count)
            .last()
            .unwrap_or(0)
    }

    /// Machine-run milliseconds within `[from, to)`: the overlap of the
    /// run-stretches of the A step function with the range.
    fn run_ms(&self, from: u32, to: u32) -> u32 {
        let mut run_ms = 0u32;
        let mut stretch_from = from;
        let mut running = false;
        for (t_ms, is_run) in self.statuses.iter().copied() {
            if t_ms >= to {
                break;
            }
            if t_ms > from {
                if running {
                    run_ms += t_ms.max(from) - stretch_from;
                }
                stretch_from = t_ms;
            } else {
                // A status before the window only sets the entry state.
                stretch_from = from;
            }
            running = is_run;
        }
        if running {
            run_ms += to - stretch_from.max(from);
        }
        run_ms
    }

    /// Good and total verdicts within `[from, to)`.
    fn verdicts_in(&self, from: u32, to: u32) -> (u32, u32) {
        let mut good = 0u32;
        let mut total = 0u32;
        for (t_ms, is_good) in self.verdicts.iter().copied() {
            if t_ms < from {
                continue;
            }
            if t_ms >= to {
                break;
            }
            total += 1;
            good += u32::from(is_good);
        }
        (good, total)
    }
}

/// One computed window: the measured components and their inputs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowStats {
    pub t_from_ms: u32,
    pub t_to_ms: u32,
    /// Planned production time = the window length.
    pub planned_ms: u32,
    /// Machine run time within the window (from node A statuses).
    pub run_ms: u32,
    /// Parts counted within the window (node P).
    pub parts: u32,
    /// Good parts within the window (node Q).
    pub good: u32,
    /// Tap-tested parts within the window (node Q).
    pub total: u32,
    pub availability: f32,
    pub performance: f32,
    pub quality: f32,
    pub oee: f32,
}

/// Computes the components over `[t_from_ms, t_to_ms)` from the stored
/// streams; `ideal_cycle_ms` is the nominal line cadence (a line property,
/// not a scenario parameter — a slowdown scenario keeps the nominal ideal).
pub fn compute(
    inputs: &WindowInputs,
    t_from_ms: u32,
    t_to_ms: u32,
    ideal_cycle_ms: u32,
) -> WindowStats {
    let planned_ms = t_to_ms.saturating_sub(t_from_ms);
    let run_ms = inputs.run_ms(t_from_ms, t_to_ms);
    let parts = inputs
        .count_before(t_to_ms)
        .saturating_sub(inputs.count_before(t_from_ms));
    let (good, total) = inputs.verdicts_in(t_from_ms, t_to_ms);
    let availability = if planned_ms > 0 {
        run_ms as f32 / planned_ms as f32
    } else {
        0.0
    };
    let performance = if run_ms > 0 {
        (ideal_cycle_ms as f32 * parts as f32 / run_ms as f32).min(1.0)
    } else {
        0.0
    };
    let quality = if total > 0 {
        good as f32 / total as f32
    } else {
        1.0
    };
    WindowStats {
        t_from_ms,
        t_to_ms,
        planned_ms,
        run_ms,
        parts,
        good,
        total,
        availability,
        performance,
        quality,
        oee: oee(availability, performance, quality),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The D2 check: a hand-made window where the components converge with
    /// the truth by construction. Machine: idle 0-10 s, run 10-50 s, idle
    /// 50-60 s -> A over [0, 60 s) = 40/60. Parts: 3 by t=20 s, 5 by 40 s,
    /// 6 by 55 s -> the window holds 6. Verdicts: 2 good of 3 in-window.
    fn inputs() -> WindowInputs {
        WindowInputs {
            statuses: vec![(10_000, true), (50_000, false)],
            counts: vec![(20_000, 3), (40_000, 5), (55_000, 6)],
            verdicts: vec![(15_000, true), (30_000, false), (45_000, true)],
        }
    }

    #[test]
    fn fixed_window_matches_the_hand_computed_truth() {
        let ideal = 5_000; // a part every 5 s while running
        let stats = compute(&inputs(), 0, 60_000, ideal);
        assert_eq!(stats.planned_ms, 60_000);
        assert_eq!(stats.run_ms, 40_000);
        assert_eq!(stats.parts, 6);
        assert_eq!((stats.good, stats.total), (2, 3));
        assert!((stats.availability - 40.0 / 60.0).abs() < 1e-6);
        // P = 5000 ms * 6 / 40000 ms = 0.75.
        assert!((stats.performance - 0.75).abs() < 1e-6);
        assert!((stats.quality - 2.0 / 3.0).abs() < 1e-6);
        let expected = 40.0 / 60.0 * 0.75 * 2.0 / 3.0;
        assert!((stats.oee - expected).abs() < 1e-6);
    }

    #[test]
    fn half_open_boundaries_attribute_edges_exactly_once() {
        // A status, a count and a verdict exactly at the boundary t=60 s:
        // they belong to the NEXT window, not this one. (Stream order is an
        // invariant — the boundary status is appended after the last one.)
        let mut inputs = inputs();
        inputs.statuses.push((60_000, false));
        inputs.counts.push((60_000, 9));
        inputs.verdicts.push((60_000, false));
        let stats = compute(&inputs, 0, 60_000, 5_000);
        assert_eq!(
            stats.parts, 6,
            "the count at exactly 60 s is the next window's"
        );
        assert_eq!(
            stats.total, 3,
            "the verdict at exactly 60 s is the next window's"
        );
        assert_eq!(stats.run_ms, 40_000);
        // And the next window sees them.
        let next = compute(&inputs, 60_000, 120_000, 5_000);
        assert_eq!(next.parts, 3);
        assert_eq!(next.total, 1);
    }

    #[test]
    fn sub_window_sees_only_its_slice() {
        // [30 s, 50 s): run the whole time, parts 5-6 = 2 (counts before 50 s
        // = 5, before 30 s = 3), verdicts in-window: cracked at 30 s and
        // good at 45 s -> Q = 0.5.
        let stats = compute(&inputs(), 30_000, 50_000, 5_000);
        assert_eq!(stats.run_ms, 20_000);
        assert_eq!(stats.planned_ms, 20_000);
        assert_eq!(stats.availability, 1.0);
        assert_eq!(stats.parts, 2);
        assert_eq!((stats.good, stats.total), (1, 2));
        assert!((stats.quality - 0.5).abs() < 1e-6);
    }

    #[test]
    fn no_run_time_zeroes_a_and_p_but_not_q_baseline() {
        let empty = WindowInputs::default();
        let stats = compute(&empty, 0, 60_000, 400);
        assert_eq!(stats.run_ms, 0);
        assert_eq!(stats.parts, 0);
        assert_eq!(stats.availability, 0.0);
        assert_eq!(
            stats.performance, 0.0,
            "no run time -> P = 0 (guarded divide)"
        );
        assert_eq!(stats.quality, 1.0, "no verdicts -> the Q = 1.0 baseline");
        assert_eq!(stats.oee, 0.0);
    }

    #[test]
    fn before_the_first_status_the_machine_is_not_running() {
        // A window entirely before the first status: idle by convention.
        let stats = compute(&inputs(), 0, 5_000, 400);
        assert_eq!(stats.run_ms, 0);
        assert_eq!(stats.availability, 0.0);
        // A window starting mid-run (after the status at 10 s): running.
        let stats = compute(&inputs(), 20_000, 30_000, 400);
        assert_eq!(stats.run_ms, 10_000);
    }

    #[test]
    fn performance_is_capped_at_one() {
        // 11 parts in 40 s of run at a 5 s ideal would exceed 1.0 (say the
        // run time is undershot by A noise) — capped to the standard bound.
        // The last count sits just inside the window (half-open ranges).
        let inputs = WindowInputs {
            statuses: vec![(0, true)],
            counts: vec![(39_999, 11)],
            verdicts: vec![],
        };
        let stats = compute(&inputs, 0, 40_000, 5_000);
        assert_eq!(stats.parts, 11);
        assert_eq!(stats.performance, 1.0);
        assert_eq!(stats.quality, 1.0);
        assert_eq!(stats.oee, 1.0);
    }

    #[test]
    fn empty_window_between_events_reports_zeros() {
        // [60 s, 90 s) with no data at all: planned 30 s, nothing ran.
        let stats = compute(&inputs(), 60_000, 90_000, 400);
        assert_eq!(stats.planned_ms, 30_000);
        assert_eq!(stats.run_ms, 0);
        assert_eq!(stats.availability, 0.0);
        assert_eq!(stats.quality, 1.0);
    }
}
