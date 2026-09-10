# Gaps in determinism and channel-independence tests

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #7
- **Location**: `line-simulator/tests/determinism.rs`, `belt.rs:179-193`

## Summary

- (a) The headline contract "requesting one channel does not change the others" is not verified end-to-end: there is no test of "`--out a --taps-dataset t`" vs "`--out b`" → `a == b`. The existing test `belt_stream_is_independent_of_the_tap_stream` only checks that the streams *differ* — necessary but not sufficient for independence.
- (b) Bit-determinism of `--dataset` / `--taps-dataset` / `--belt-events` is not verified at the binary level (unit tests exist, no integration test).
- (c) `determinism.rs:24-25` — fixed file names in `/tmp` without cleanup: two parallel test runs (two checkouts, one CI runner) collide and flake.

## Suggested fix

- A combined-run CLI test (a) — it will also be a regression test for any future RNG consumption changes.
- An integration bit-determinism test for all modes (b).
- `tempfile`/process-unique names for temp files + removal after comparison (c).
