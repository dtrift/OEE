# `parity_gen` duplicates the input quantization formula from interp

- **Severity**: important
- **Component**: ml-exporter
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A5, Important #5
- **Location**: `ml/exporter/src/bin/parity_gen.rs:45-48` vs `ml/exporter/src/interp.rs:102-108`; also `parity_gen.rs:66-68`

## Summary

The same code `(v/scale).round_ties_even() + zp … clamp` lives in two places. Under drift (say, the rounding mode is changed only in interp) the fixtures are generated with input A while interp computed its expectations with input B — the parity test measures the wrong pair, failing confusingly or passing falsely.

Also: `io_quants(&interp)` ignores its parameter and re-reads the file from disk.

## Suggested fix

- Expose `pub fn quantize_input(&self, values: &[f32]) -> Vec<i8>` (or return the quantized input from `run`) and reuse it in `parity_gen`;
- make `io_quants` a method of `InterpModel`.
