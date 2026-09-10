# The "advance events by the clock" loop is duplicated 5 times

- **Severity**: important
- **Component**: line-simulator (also affects nodes and oee-aggregator tests)
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #6
- **Location**: `line-simulator/src/main.rs:67-75`, `taps.rs:88-91`, `belt.rs:65-68`; copies in other crates: `nodes/tests/node_pipeline.rs:45-67`, `oee-aggregator/tests/experiment.rs:58-70` (the `{:.4}` formatting is duplicated there too)

## Summary

The semantics of `event.t_ms <= t` (inclusive) is implemented by hand in five places, including the CSV format. Under any change (e.g. the f32-time fix), a desynchronization between `main.rs` and other crates' tests would produce a CSV that does not match the "truth", and it would be hard to notice.

## Suggested fix

Export a shared helper from the lib:

- `Simulator::run(&scenario) -> impl Iterator<Item = Sample>`, or
- `write_raw_csv(scenario, seed, writer)`,

and use it in `main.rs` and the `nodes`/`oee-aggregator` tests. After that — a CLI integration test for channel independence (see `20260909120014-important-determinism-test-gaps-line-simulator-bl.md`).
