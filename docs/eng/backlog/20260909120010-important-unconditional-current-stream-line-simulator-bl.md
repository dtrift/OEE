# The current stream is generated unconditionally

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #3
- **Location**: `line-simulator/src/main.rs:62-75`

## Summary

The synthesis loop over 69.12M samples and the `Vec::with_capacity` (~830 MB on soak-1m) always run, even when only `--taps-dataset` / `--belt-events` are requested and do not need that vector.

## Why it matters

- On soak scenarios: hundreds of MB of memory and noticeable time wasted.
- With unrealistic `duration_ms`, `with_capacity` ends in an allocator abort instead of a comprehensible message.

## Suggested fix

Build `samples` only when `args.out.is_some() || args.dataset.is_some()`.

Fix together with CSV buffering (see `20260909120011-important-csv-buffered-in-memory-line-simulator-bl.md`): conditional generation + streaming writes will fit soak runs into memory comparable to a single artifact's size.
