# Week 3 Gate — Conv1D Parser + Codegen + ML Pipeline

> Implemented 2026-08-28 per [decompose/step-3.md](decompose/step-3.md) on branch
> `feat/parser-codegen-ml` (OEE) and `feat/parser-codegen-ml` (fork). All checks re-run at
> formalization time: fork — 35 lib + 7 integration suites, clippy clean; workspace — 11 suites,
> clippy clean.

## Gate checklist

| Gate item                                                         | Status | Artifact / check                                                                                                                                                                 |
| ----------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `ml/models/conv1d.tflite` builds through `#[model]`, host predict | yes    | [conv1d_model.rs](../../fork/microflow/tests/conv1d_model.rs) — 3 tests; [conv1d_spike.rs](../../fork/microflow/examples/conv1d_spike.rs) — `cargo run` green                    |
| 18 ops fold into 6 layers (§2.1, DoD §6)                          | yes    | [shape_fold.rs](../../fork/microflow/microflow-macros/src/shape_fold.rs) — `folds_spike_graph_to_six_layers` on the real file: CONV_2D, AVG_POOL, CONV_2D, AVG_POOL, FC, SOFTMAX |
| Rank-3 input → `Buffer2D<f32, 128, 1>` user API (§2.2)            | yes    | The expansion in `target/microflow-expansion.rs`; the example passes an `SMatrix<f32, 128, 1>`                                                                                   |
| FC per-channel + optional bias (§3.3)                             | yes    | [fully_connected.rs](../../fork/microflow/src/ops/fully_connected.rs) — generic `WEIGHTS_QUANTS`, 2 runtime tests; the spike model's FC has 4 scales and no bias and runs        |
| Parity vs tf.lite.Interpreter, ±1 quant (§5.3)                    | infra  | [conv1d_parity.rs](../../fork/microflow/tests/conv1d_parity.rs) + [dump_parity_fixtures.py](../../ml/scripts/dump_parity_fixtures.py); **awaits a TF-venv run** (see Deviations) |
| Node A model trained on synthetic data, full-int int8, metrics    | infra  | [train_model_a.py](../../ml/scripts/train_model_a.py) + the dataset pipeline; **awaits a TF-venv run**                                                                           |
| Golden features: Rust vs numpy (plan section 6)                   | yes    | [golden_features.rs](../../features-cli/tests/golden_features.rs) vs [golden_features.py](../../ml/scripts/golden_features.py) — zero-crossings bit-for-bit, floats ±1e-6        |
| Workspace → fork bridge (D6)                                      | yes    | [a.rs](../../nodes/src/a.rs) — `#[model]` compiles inside the workspace and predicts; `[patch.crates-io] nalgebra` enabled in the root `Cargo.toml`                              |

## What was built (per day of the decomposition)

- **D1 — shape-folding parser (§2.1–2.2)**: `microflow-macros/src/shape_fold.rs`. A
  tensor→virtual-shape table walks the operator list once: `EXPAND_DIMS`/`RESHAPE` fold into
  virtual reshapes, the Flatten chain `SHAPE → STRIDED_SLICE → PACK → RESHAPE` is evaluated
  statically (`-1` resolution, Python-like slicing, shrink-axis), real operators keep effective
  rank-2/4 shapes. Anything unfoldable aborts with the op index, shapes, and the expectation.
- **D2 — operators (§2.3–2.4) + codegen (§4)**: `ops/conv_1d.rs` (macros) emits the week-2
  kernel call with the bias converted to accumulator units and geometry asserts
  (`h == 1`, `T ≥ k`, output-length match); `fully_connected` (runtime) grew per-channel
  `WEIGHTS_QUANTS` with a 3-tuple of per-output constants and the macro synthesizes a zero bias
  for the converter-dropped one (F6). Rank changes between layers become runtime `reshape`s.
  `conv1d.tflite` was added to `fork/microflow/models/` (dense_spike precedent).
- **D3 — real model through `#[model]`**: `examples/conv1d_spike.rs` +
  `tests/conv1d_model.rs` (softmax sanity, determinism, quantized-entry smoke). The expansion
  reads as spec §4 intends: user `Buffer2D<f32, 128, 1>` in, `Tensor4D<i8, 1, 1, 128, 1, 1>`
  inside, per-channel filters (8 scales — F5 confirmed), the folded Flatten as one
  `Tensor2D<_, 1, 480, 1>` reshape, per-channel FC weights (4 scales — F6 confirmed).
- **D4 — features + dataset**: `features-cli/src/features.rs` (RMS, peak, zero-crossings,
  Goertzel spectrum at 50/150/250 Hz — no_std, fixed bins); `line-simulator/src/dataset.rs`
  (`--dataset` CLI mode: uniform-state windows of 128 with stride, CSV `label,state,x000..x127`,
  class histogram). A `scenarios/jam_cycle.toml` was added — the week-2 scenarios keep jam at
  ~1% of windows, too thin to train on.
- **D5 — training script**: `ml/scripts/train_model_a.py` (seeded, class-weighted, full-int8
  PTQ, ops dump, int8 confusion matrix, val split export for parity).
- **D6 — bridge**: `nodes` depends on `fork/microflow` by path (+ `nalgebra` via the duplicated
  `[patch.crates-io]`); `cargo test` green in both workspaces.
- **D7 — this gate.**

## Deviations from the decomposition

- **No TensorFlow in the implementation sandbox** (Python 3.14 + numpy only, network blocked):
  the interpreter parity run (D3.2/D6) and the training run (D5) could not be executed here.
  Both sides are delivered and everything runnable was run: the parity harness is validated by
  a round-trip self-test (`fixture_parser_roundtrip`, zero diff), the training script is
  syntax-checked and modeled on the proven `build_conv1d_model.py`. User actions, in order:
  1. `tmp/venv312/bin/python ml/scripts/dump_parity_fixtures.py` → `cargo test -p microflow --test conv1d_parity` (closes the parity gate item);
  2. generate datasets: `cargo run -p line-simulator -- --scenario scenarios/<s>.toml --seed <n> --dataset tmp/ds_<s>_<n>.csv`;
  3. `tmp/venv312/bin/python ml/scripts/train_model_a.py --datasets tmp/ds_*.csv` → `ml/models/model_a.tflite` + metrics; then point `nodes/src/a.rs` at it and re-run the parity dumper with `--model ml/models/model_a.tflite --windows-npz ml/models/model_a_val.npz`.
- **A latent week-2 bug surfaced**: `conv_1d.rs` used `f32::round_ties_even()`, which is
  std-only in this toolchain's `no_std` core (the week-2 gate ran on cached build artifacts, so
  it never failed there). Replaced with a `libm::truncf`-based helper with identical semantics —
  the week-2 golden fixtures (96 cases, bit-for-bit) still pass, pinning the equivalence.
- `microflow-macros/src/ops/reshape.rs` was removed (moved to `tmp/trash/`): folding supersedes
  it; plain rank-2/4 reshapes now emit through the same conversion path.
- Compile-fail tests (D2.4) would need a `trybuild` dev-dependency (not fetchable offline).
  The substance is covered another way: the fold returns `Result` with op-indexed messages
  (unit-tested on the pure helpers), and a genuinely misrouted model was caught during
  development with a readable abort (the `person_detect` 1×1-filter case).
- Dataset balance: honest state — per seed, the four scenarios give roughly
  `[idle 1136, run 4165, jam 435, overload 336]` windows; training uses inverse-frequency class
  weights and reports the confusion matrix. A truly balanced dataset would need either
  jam-heavy scenarios or window-stratified sampling (week-4 material, see Retro).
- The parity fixture path convention: `#[model]` paths resolve relative to the **workspace
  root** during compilation (rustc cwd), while test binaries run with the package directory as
  cwd — `nodes/src/a.rs` uses `ml/models/conv1d.tflite`, the fork's own tests use
  `models/conv1d.tflite`. Recorded in NOTES.

## Risks (plan section 11)

- Risk #2 (the parser is harder than expected) — retired: the real graph folds as spec'd,
  including the dynamic Flatten chain.
- "Too-good models" — the training script warns above 99.5% val accuracy and the scenarios
  carry drift/noise parameters; final judgment waits for the actual training run.
- The `abort_call_site!` messages carry op index + shapes + expectation (§2.1) — verified by
  construction (a `person_detect` 1×1-filter conv initially misrouted to the 1-D path; the
  error was readable and led to the input-height half of the discriminator).

## Decision for week 4

Per the plan: node A end-to-end (sim → features → predict → MQTT) on the trained
`model_a.tflite`, node Q (sound synthesis + model), and the cut-line check at the end of the
week. Inputs ready: the bridge, the dataset pipeline, the training script, the parity tooling.

## Retro

- Hours: not tracked per session (a recurring gap — week 2 had the same note). The wall-clock
  for this implementation pass was a single long session; the estimate was ~16–19 h.
- The cached-artifacts trap (week-2 kernel not actually compiling in this toolchain) cost a
  detour; lesson recorded in NOTES: force a clean rebuild of touched crates before trusting
  green caches (`touch <file> && cargo build` is not enough when the failure is older than the
  cache — use `cargo clean -p <crate>` when in doubt).
- Two rounds of hand-computed unit-test expectations were wrong (window counting in
  `dataset.rs`, Python slice semantics in `strided_slice`) — both were my arithmetic, both
  caught by the first run. Same lesson as week 2: compute expected values from first
  principles, then double-check against a second method (Python) before writing the assert.
