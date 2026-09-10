# CSV is assembled entirely into a `String` before writing

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #4
- **Location**: `line-simulator/src/taps.rs:144-156`, `dataset.rs:58-69`, `belt.rs:109-121`

## Summary

The whole file is first accumulated in memory (for the soak-1m taps dataset that is ~1 GB of string on top of ~1.2 GB of `Vec<TapEvent>` with windows), then a single `write_all`.

Peak memory ~2× the artifact plus the event vector itself — for the declared ~1 GB artifacts this already affects machine choice.

## Suggested fix

- Line-by-line writing through `BufWriter` (or `csv::Writer`, as in `write_raw`).
- Optionally: do not store `samples` in `TapEvent`, write the window on the fly.

Fix together with `20260909120010-important-unconditional-current-stream-line-simulator-bl.md`.
