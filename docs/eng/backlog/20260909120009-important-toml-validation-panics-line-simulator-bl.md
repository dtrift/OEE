# Panics from user TOML in the hot loop

- **Severity**: important
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, Important #2
- **Location**: `signal.rs:56, 67`, `taps.rs:118-119, 127`; missing validation: `scenario.rs:170-175` (`Noise`), `scenario.rs:204-222` (`Envelope`), partial `Taps::validate` (`scenario.rs:65-91`)

## Summary

`Noise.sigma_a`, `Signal.drift_sigma`, `Taps.noise_sigma`/`crack_noise_boost` are not validated at all:

- `noise.sigma_a = -0.1` (or `nan`/`inf` — TOML parses them) passes parsing and panics on the very first sample via `.expect("sigma >= 0")` (`signal.rs:67`);
- a negative `amp_jitter`/`freq_jitter` produces an empty range `random_range(0.2..=-0.2)` → a panic inside `synth_window`;
- negative `envelope.*` values are silently accepted (the signal flips sign).

This is the "raise-in-loop kills the whole batch" pattern the project tries to avoid. `belt` is validated exemplarily while neighboring sections are not; the asymmetry itself is a source of errors.

## Suggested fix

Add validation to `Scenario::parse` (`scenario.rs:266-272`) following the `Belt::validate` template:

- `sigma_a`/`drift_sigma`/`noise_sigma` — finite and ≥ 0;
- `amp_jitter`/`freq_jitter` ∈ [0, 1);
- `crack_noise_boost` ≥ 0;
- `envelope.*` ≥ 0;
- `signal.third`/`fifth` — finite.

After that the `expect`s become unreachable — replace with an `unwrap_or` branch or keep with a "validated in parse" comment.
