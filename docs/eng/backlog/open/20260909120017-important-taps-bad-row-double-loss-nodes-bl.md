# One corrupt taps-dataset row costs two lost verdicts

- **Severity**: important
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A2, Important #4
- **Location**: `nodes/src/sim_source.rs:215-234` + `nodes/src/q.rs:69-91`

## Summary

A taps-dataset row is an atomic window (1024 samples), but the dirty flag from skipped row M is set at the moment the **first sample of row M+1** is emitted → the `push(None)` marks dirty the window that is being filled with good samples of row M+1, and that window is discarded.

Result: `verdicts = truth − 2·(bad_rows)` — a false miss of a good part. The "poisons the surrounding window" rationale was written for node A's continuous signal (correct there); for Q, where "parts are independent events", isolation must be per-row.

## Suggested fix

- Give `TapSource` a notion of a row boundary (e.g. `next_tap()` → `Option<...>` in `run_q`, dirty at the row level);
- or reset the accumulator's dirty flag at the window/row boundary.

## Tests

`run_q` with a corrupt row in the middle (currently only a source test exists, and the corrupt row there is the first one).
