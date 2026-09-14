# f32 time: `t_ms` and phase degradation on long runs

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #1
- **Location**: `line-simulator/src/lib.rs:59-66`, `signal.rs:52, 64-66`

## Summary

Both `t_ms` and the phase are computed via `sample_index as f32 / 1600.0`. f32 has a 24-bit mantissa: from ~2^24 samples (**~2.9 h**) onward, `sample_index as f32` rounds, and neighboring indices produce the same `t`. By 12 h (soak-1m, 69.12M samples) the index ULP is 8: `t_ms` repeats up to 8 times in a row, then jumps by ~5 ms; `sin(2π·50·t)` gets an argument of ~1.4e7 — a staircase instead of a sine; the drift period detection `(t*50) as u64` (`signal.rs:52`) drifts too.

Acknowledged by the authors in `soak-1m.toml:7-14` (node A "flapped" 600–4800 times/hour from ~the 14th hour of a 30-hour run), but the knowledge lives only in a TOML comment — no guard or warning in the code.

Additionally: `f32::sin` with large arguments depends on the argument-reduction quality of the system libm — cross-platform bit-identity is not guaranteed.

## Suggested fix

1600 Hz / 50 Hz = **exactly 32 samples per period** (noted in `scenario.rs:8`):

- `t_ms = (sample_index as u64 * 1000) / 1600` — exact integer arithmetic;
- phase: `x = (sample_index % 32) as f32 / 32.0`, then `sin(2π·x)`, `sin(2π·3x)`, `sin(2π·5x)` (arguments ≤ 2π·5; a 32-entry LUT is possible — also removes the libm dependency);
- drift period: `sample_index / 32`.

The "≤12 h" limitation disappears.

## Note

Output bits will change — regenerate datasets/artifacts as a separate commit and re-pin the test baselines. Move the f32-ceiling knowledge from `soak-1m.toml` into the crate docs (README/CHANGELOG).
