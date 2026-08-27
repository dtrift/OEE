# Week 2 — decomposition (kernel `Conv1D` + current signal)

> Branch: `feat/conv1d-kernel` — in both repos (fork/microflow and OEE).

> Breakdown of the "Week 2" row of the plan [`plan.md`](../plan.md), plan section 9. The
> main track is the int8 kernel `conv_1d` per the week 1 spec
> ([`fork/docs/conv1d-spec.md`](../../../fork/docs/conv1d-spec.md), §3 "Kernel", §5 "Tests")
> with bit-for-bit golden tests against the Rust reference; the second is bringing the
> simulator's current signal up to ML grade. Mode: 1 person; weekdays ~2–3 h, Saturday
> ~4 h. Estimate: ~16–19 h.
>
> Input: the week 1 gate is closed — [`week1-gate.md`](../week1-gate.md). The baseline is
> above plan: the simulator already produces a deterministic CSV (50 Hz + 3rd/5th
> harmonics + envelope + noise — `line-simulator/src/signal.rs`), so this week's simulator
> track is parameterization and separability, not synthesis from scratch.
>
> Detailing of the draft [`plan/step-2.md`](../plan/step-2.md) based on week 1 facts;
> on conflict → this file is edited, the plan only in substance.

## Week gate (minimum done)

- [x] The `conv_1d` kernel passes the toy test (§5.1) and the golden cases bit-for-bit
      against the Rust reference (§5.2): int8 input, i32 accumulator, per-channel requant.
      8 unit tests with hand-computed numbers —
      [conv_1d.rs](../../../fork/microflow/src/ops/conv_1d.rs); 96 golden cases —
      [conv1d_golden.rs](../../../fork/microflow/tests/conv1d_golden.rs)
      (generator [golden_gen.rs](../../../fork/microflow/examples/golden_gen.rs),
      seed 42, idempotent).
- [x] Edge cases green (DoD §6): `T < kernel_size` (same), `stride = 2`, `valid/same`,
      8 channels; the kernel is `no_std`, no allocations. Beyond the list: T=1/63/64,
      saturation, ties-to-even, zp_x padding neutrality — the unit and golden sets.
- [x] `cargo test` + `cargo clippy --all-targets -- -D warnings` green in the fork and
      the workspace. Fork: 33 lib + 4 integration, clippy clean (the CI flag
      `-A mismatched_lifetime_syntaxes` for generated flatbuffers); workspace: 10 suites,
      clippy clean; re-checked at the gate — [week2-gate.md](../week2-gate.md).
- [x] Simulator: signal parameters in the scenario; the classes are separable but not
      perfectly (RMS windows differ across modes, tails overlap). The `[signal]` section —
      [scenario.rs](../../../line-simulator/src/scenario.rs); separability tests (mean RMS
      values differ, spread within a mode) — [signal.rs](../../../line-simulator/src/signal.rs);
      scenarios [base](../../../scenarios/base.toml)/[downtime](../../../scenarios/downtime.toml)/[degradation](../../../scenarios/degradation.toml);
      one seed → an empty `diff` (a CLI check in the gate).

## Day-by-day summary

| Day | Session topic | Essence                                           | Artifact                           |
| --- | ------------- | ------------------------------------------------- | ---------------------------------- |
| D0  | engine        | branches in both repos, re-read spec §3/§5        | branches ready, questions resolved |
| D1  | engine        | kernel skeleton: int8 dot + int32 acc             | acc-level toy test green           |
| D2  | engine        | per-channel requant, geometry, fused RELU         | full §5.1 toy test green           |
| D3  | tests         | reference + `golden-gen`, ~100 cases              | golden set bit-for-bit green       |
| D4  | engine        | cleanup, clippy, kernel review                    | clean diff; buffer → §3.3 FC       |
| D5  | simulator     | signal parameters from the scenario               | TOML: harmonics, variability       |
| D6  | simulator     | separability "not perfect", dataset seed material | CSV + separability check           |
| D7  | gate          | checklist, retro (with hours), week 3 plan        | `docs/week2-gate.md`               |

## D0 (evening before start, ~0.5 h): branches and spec

1. Branches `feat/conv1d-kernel`: in `fork/microflow` (kernel) and in OEE (simulator +
   the submodule pointer bump at the end of the week).
2. Re-read spec §3 (kernel) and §5 (tests); write out and resolve ambiguities before
   code — the "spec → code → tests" rule (plan section 8).

## D1 (Mon, ~2–3 h) — engine: kernel skeleton

1. `fork/microflow/src/ops/conv_1d.rs` per spec §3.1: input `Tensor4D` `(1,1,T,C)`,
   OHWI weights `(F,1,k,C)` per-channel, bias `(F,)` int32; register the module in the
   runtime next to `conv_2d`. Calling it from the macro is week 3: for now only a
   runtime function.
2. Convolution core: int8 dot product over the k window, i32 accumulator — without
   requant.
3. An accumulator-level toy test: 1 channel, k=3, T=5, stride 1, valid; the acc values
   are hand-computed (int32, no quant transitions) — a bit-for-bit cross-check.

Check: the acc-level toy test is green; the kernel compiles `no_std`-compatibly,
without allocations (following the `conv_2d` pattern).

## D2 (Tue, ~2–3 h) — engine: requant, geometry, fused RELU

1. Per-channel requant per §3.1: `m(f) = (scale_x·scale_w[f])/scale_out`; output =
   `clamp_i8(round_ties_even(acc·m(f)) + zp_out)`. Not "mul → round → shift": the spec
   fixed an f32 multiplier; `round_ties_even` rounding — identical in the kernel and
   the reference.
2. Fused RELU (a CONV_2D option) — in quantized coordinates: `max(x, zp_out)`.
3. Geometry §3.2: `T_out = floor((T−k)/stride)+1` (valid), `ceil(T/stride)` (same);
   `same` padding is filled with `zp_x`, not zero; `T < k` with valid — rejected at the
   codegen level (week 3), in the kernel a `debug_assert`; with `same`, windows are
   padded up with `zp_x`.
4. Tests: the full §5.1 toy test (zp/scale hand-computed — bit-for-bit); `stride = 2`,
   8 channels, valid/same; one output row cross-checked against a calculator.

Check: unit tests green; every requant step explainable in words.

## D3 (Wed, ~3 h) — tests: golden against the Rust reference [gate core]

1. A reference Conv1D in the fork's test infrastructure (`fork/microflow/tests/`): a
   naive implementation strictly per §3.1 — int8 input/weights, i32 accumulator, requant
   with `round_ties_even`. Not float end-to-end: step-by-step int8 semantics, otherwise
   the test is pointless.
2. The `golden-gen` generator (a bin in the fork, e.g. `src/bin/golden-gen.rs`): §5.2
   cases — T 1–64, channels 1–8, kernel 1–7, stride 1–2, valid/same, ~100 of them, seed
   fixed; fixtures — human-readable files in the repo (e.g.
   `fork/microflow/tests/golden/`), regenerated with one command.
3. The kernel test: reads the fixtures, cross-checks the output bit-for-bit — integer
   semantics are deterministic, no tolerance needed.

Check: all cases green; a mismatch is a bug on one of the sides: investigate by
operation (accumulate → requant), not by tuning a tolerance.

## D4 (Thu, ~2 h) — engine: cleanup, review, buffer

1. clippy/fmt/doc comments; cross-check the structure against `conv_2d` — runtime API
   consistency.
2. Review of the kernel diff: the "why this way" question for every function.
3. Buffer: if D1–D3 ran long — finish off the golden cases. If ahead of schedule —
   §3.3: per-channel `fully_connected` + optional bias (risk #2 materialized, our graph
   needs it); not done — week 3, the gate does not block on it.

## D5 (Fri, ~2–3 h) — simulator: signal parameters from the scenario

1. Harmonics out of the code and into TOML: currently 3rd/5th are hardcoded in
   `signal.rs` (0.15/0.07) → the scenario's `[signal]` section (harmonic coefficients).
2. Within-mode variability: the Run amplitude wanders around the nominal value (seeded),
   otherwise the classes are too clean for ML (the "too-good models" risk, plan
   section 11).
3. Determinism preserved: the `deterministic_csv` test is green after the edits.

Check: `scenarios/base.toml` defines the whole signal; the modes are distinguishable
by eye.

## D6 (Sat, ~3–4 h) — simulator: separability "not perfect"

1. The noise/variability level — scenario parameters; several seeds → different
   datasets, one seed → a bit-for-bit identical CSV.
2. A separability check: RMS of windows (e.g. 128 samples — the future CNN input)
   across modes — the means differ, the distributions overlap. A rough script or test,
   no ML.
3. Dataset seed material for weeks 3–4: 2–3 scenarios (normal / downtime / degradation)
   with different seeds.

Check: "distinguishable, but not 100%" — otherwise ML is pointless (plan section 4);
two runs with one seed → an empty `diff`.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; every "yes" — backed by a link to an artifact.
2. Retro: hours vs estimate — record each session's time as the week goes (the
   agreement from [`week1-gate.md`](../week1-gate.md)); what slowed things down.
3. Week 3 plan: parser + codegen + training (draft — [`step-3.md`](../plan/step-3.md)).

Artifact: [`docs/week2-gate.md`](../week2-gate.md) — checklist + 5–10 lines of
conclusions (the early plan listed `tmp/OEE/week2-gate.md`; `tmp/` is gitignored —
gate docs live in `docs/`, precedent — week 1).

## Escalation points

- The kernel and the reference diverge → cross-check the requant operation order, the
  rounding mode (`round_ties_even` on both sides, per the spec) and the `same` padding
  value (`zp_x`) — two common causes.
- A shared misreading of the spec (the bug is in both the reference and the kernel) →
  the week 3 safety net: a cross-check against a real `.tflite` via the TFLite
  interpreter (§5.3).
- The spec turns out inaccurate during implementation → fix the spec first, then the
  code ("spec → code → tests"): the spec is the contract for weeks 2–3.
- The kernel slips past D4 → cut the simulator track: D5–D6 move to weekdays of week 3.
  The week's gate is the kernel, the simulator is the second track.

## Anti-scope (what we do NOT do in week 2)

- The `#[model]` parser and codegen, calling the kernel from the macro — week 3.
- Per-channel `fully_connected` + optional bias (§3.3) — only the D4 buffer; otherwise
  week 3.
- Model training — weeks 3–4 (the dataset is still accumulating).
- Kernel optimizations (SIMD, loop unrolling) — after correctness; the numbers — week 6.
