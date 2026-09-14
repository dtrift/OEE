# Interp panics on foreign/invalid `.tflite` instead of returning `Err`

- **Severity**: important
- **Component**: ml-exporter
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A5, Important #2
- **Location**: `ml/exporter/src/interp.rs:318, 373, 433, 447, 497` (`sx[0]`/`zpx[0]`), `interp.rs:347, 532` (`repeat(&sb[0])`), `interp.rs:77, 96, 241-242` (`unwrap` on `subgraph.inputs()`); likewise `dumper.rs:30-36`

## Summary

- `sx[0]`/`zpx[0]` on a tensor without quantization (e.g. a float32 model) → index-out-of-bounds;
- `repeat(&sb[0])` panics on an empty bias-scale vector;
- `.unwrap()` on `subgraph.inputs()`.

A tool whose job is reading arbitrary files (including TF-converted ones) must return `Err`, not kill the process.

## Suggested fix

- A helper `fn per_tensor_quant(t: &Tensor) -> Result<(f32, i64), String>` with a length check;
- `inputs().ok_or("the subgraph declares no inputs")?`;
- the same for `dumper.rs`.
