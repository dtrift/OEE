# "Re-run is bit-identical" is not locked in by an automated test

- **Severity**: important
- **Component**: ml-trainer
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A5, Important #3
- **Location**: claim — `ml/README.md:10, 73-79` ("D6 double-run sha256 gate"); tests — component-level only (`writer_output_is_deterministic`, `float_format_roundtrip`, split/calibration determinism)

## Summary

Full-pipeline determinism is a manual procedure (D6). A minor burn release or a refactor will silently break the property promised in README.

## Suggested fix

An integration test in trainer:

- a synthetic CSV (~40 windows);
- two `pipeline::run` invocations (2 epochs, temp dirs);
- `assert_eq!` on `report.sha256` and the `.float` bytes.

Fast, and it closes the README claim for good.
