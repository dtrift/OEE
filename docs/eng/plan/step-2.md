# Week 2 — decomposition (`Conv1D` kernel + current signal)

> Branch: `feat/conv1d-kernel` — in both repos (OEE and fork/microflow)

> Decomposition of the "Week 2" row from [`plan.md`](../plan.md),
> section 9. The main line is the int8 `Conv1D` kernel per the week 1 spec
> (`fork/docs/conv1d-spec.md`) with golden tests against a Rust reference; the second is a full-fledged
> current signal in the simulator. Mode: 1 person; weekdays ~2–3 h, Saturday ~4 h.
> Estimate: ~16–19 h. Entry: the week 1 gate is green — the fork builds, the spec exists.

## Week gate (minimum ready)

- [ ] The `Conv1D` kernel passes golden tests against the Rust reference (int8, int32 accumulator, requant).
- [ ] Edge cases green: `T < kernel_size`, `stride = 2`, `valid/same` padding.
- [ ] `cargo test` + `cargo clippy -- -D warnings` green in the fork and the workspace.
- [ ] Simulator: the current signal by FSM mode + harmonics + noise; the classes are distinguishable by eye.

## Day-by-day summary

| Day | Session topic | Summary                                       | Artifact                         |
| --- | ------------- | --------------------------------------------- | -------------------------------- |
| D1  | engine        | kernel skeleton: int8 dot + int32 accumulator | toy test green                   |
| D2  | engine        | per-channel requant, stride/padding           | geometry unit tests              |
| D3  | tests         | golden from Rust `golden-gen`, ~100 cases     | the golden set is green          |
| D4  | engine        | cleanup, clippy, kernel review                | a clean diff                     |
| D5  | simulator     | envelope by mode + 50 Hz + harmonics          | CSV with modes                   |
| D6  | simulator     | seeded noise, class separability              | CSV "distinguishable, not ideal" |
| D7  | gate          | checklist, retro, the week 3 plan             | `tmp/OEE/week2-gate.md`          |

## D1 (Mon, ~2–3 h) — engine: the kernel skeleton

1. Per the spec (the "Kernel" section): structures — weights, biases, per-channel scale/zero-point.
2. The convolution core: int8 dot-product + int32 accumulator, without requant subtleties.
3. Toy test: 1 channel, kernel 3, the output computed by hand — a cross-check.

Check: the toy test is green; the kernel compiles in a `no_std`-compatible way.

## D2 (Tue, ~2–3 h) — engine: requant and geometry

1. Per-channel requant: `mul → round → shift`, the input/output zero-points.
2. Geometry: stride, `valid/same` padding; multi-channel support.
3. Tests: `stride = 2`, 2+ channels, convolution with padding; one output row
   is verified manually with a calculator.

Check: unit tests green; every requant step can be explained in words.

## D3 (Wed, ~3 h) — tests: golden against the Rust reference [the core of the gate]

1. A reference Conv1D in Rust (a naive implementation per the spec, the same toolchain as the kernel):
   int8 input/weights, int32 accumulator, requant; the rounding mode is explicit
   (`f32::round_ties_even`), fixed in the spec.
2. The `golden-gen` case generator (a bin next to the kernel tests): T 1–64, channels 1–8,
   kernel 1–7, stride/padding — ~100 cases; fixtures are readable files in the repo,
   regenerated with one command.
3. The kernel test: reads the golden files, cross-checks the output bit-for-bit — the integer
   semantics is deterministic, a ±1 tolerance is not needed.

Check: all cases green; a discrepancy is a bug in the kernel or in the reference —
investigate operation by operation (accumulation → requant), not by tuning a tolerance.

## D4 (Thu, ~2 h) — engine: cleanup and review

1. clippy/fmt/doc comments in the kernel; remove the spike's temporary code.
2. Review of the kernel diff: the question "why this way" for every function.
3. Buffer: if D1–D3 dragged on — finish the golden tests here.

## D5 (Fri, ~2–3 h) — simulator: the current signal

1. The amplitude envelope by FSM mode (section 4): idle / run / jam / overload.
2. A 50 Hz carrier + 2–3 harmonics; the amplitudes are scenario file parameters.
3. CSV: time, current, the true mode (ground truth).

Check: the CSV plot is readable, the modes are visually distinguishable.

## D6 (Sat, ~3–4 h) — simulator: noise and separability

1. Seeded (Gaussian) noise on top of the signal; the noise level is a scenario parameter.
2. Varying the parameters with the seed — insurance against "too-good models"
   (the section 11 risk): the classes are distinguishable, but not perfectly.
3. Determinism: two runs with the same seed → `diff` empty.

Check: distinguishable by eye, but not 100% — otherwise ML is pointless (plan section 4).

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; every "yes" — with a link to the artifact.
2. Retro: hours vs estimate; what slowed things down.
3. The week 3 plan: parser + codegen + training.

Artifact: `tmp/OEE/week2-gate.md`.

## Escalation points

- The kernel and the reference disagree → cross-check the requant operation order and the rounding mode
  (round-half-to-even on both sides, per the spec) — a common cause.
- A shared misreading of the spec (a bug in both the reference and the kernel) → the week 3 insurance:
  a cross-check against a real `.tflite` via the TFLite interpreter.
- The reference must not compute in float "all the way through" and quantize at the output — only
  the step-by-step int8 semantics of the spec, otherwise the test loses its meaning.
- Week 1 collapsed (the fork didn't build) → this week becomes week 1, shifted:
  the spec takes priority over the kernel, the kernel moves to week 3.

## Anti-scope (what we do NOT do in week 2)

- The `#[model]` parser and codegen — week 3.
- Model training — weeks 3–4 (the dataset is still accumulating).
- Kernel optimizations (SIMD, unrolling) — after correctness; the numbers — week 6.
