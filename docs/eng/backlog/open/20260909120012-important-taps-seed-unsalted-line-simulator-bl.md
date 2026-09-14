# The taps channel seed is not salted against the current channel

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #5
- **Location**: `line-simulator/src/taps.rs:81` vs `lib.rs:46`

## Summary

Both channels use `StdRng::seed_from_u64(seed)` without a salt. Belt is salted (`belt.rs:43, 57`) because its RNG consumption pattern coincides with taps (a `random::<f32>()` per slot) and the streams would mirror each other. Taps and current currently consume differently (uniform vs ziggurat-Normal), so there is no mirror — but independence rests precisely on the consumption patterns not coinciding.

The "three independent streams" contract (README) holds by circumstance, not by construction: refactoring the draw order/kinds can silently introduce a correlation.

## Suggested fix

- A per-channel salt: `TAPS_SEED_SALT` following the `BELT_SEED_SALT` pattern;
- or a single KDF: `seed_channel = hash(seed || "taps")`.

Note: output bits will change — regenerate datasets/baselines.
