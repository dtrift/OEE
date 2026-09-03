# NOTES — microflow-rs fork structure

> Week-1 working journal (D1–D2). Base: commit `6d193da` (main), microflow v0.1.3.

## D1 — building the fork: facts

- Rust: 1.96.1 (pinned in the project's `.tool-versions`, asdf/rustup).
- `cargo build --release`: ~10 s (cold cache, 17 s CPU).
- `cargo test --release`: 25 lib tests + 3 integration — all green.
- The `sine` example on the host: `Predicted sin(0.5): 0.413` vs the exact
  `0.479` (the model is a toy, the error is expected). Risk #1 (build) is
  retired.
- Build nuance: the fork depends on `nalgebra` through a **git patch**
  (`[patch.crates-io] nalgebra = { git = "https://github.com/matteocarnelos/nalgebra" }`).
  A patch from a **dependency's** manifest is **not applied** — if our
  workspace depends on the fork by path, the same `[patch.crates-io]` must be
  duplicated in the workspace root `Cargo.toml` (otherwise nalgebra comes
  from crates.io).
- Sandbox peculiarity (not about the fork): cargo is invoked through the
  `tmp/bin/cargo` wrapper with `CARGO_HOME` in `tmp/cargo-home`.

## D2 — crates and roles

| Crate              | Role                                                                                                           |
| ------------------ | -------------------------------------------------------------------------------------------------------------- |
| `microflow`        | runtime: tensor types, buffers, operators (no_std, no allocations)                                             |
| `microflow-macros` | the compiler: the `#[model]` proc-macro, tflite parsing, code generation                                       |
| `examples/`        | examples: host (`sine`, `speech`, `person_detect`) + platforms (QEMU, ESP32, Arduino), excluded from workspace |

The `microflow/src/` runtime:

- `tensor.rs` — `Tensor2D`/`Tensor4D` (type, shape, scale/zero-point, the
  quant set); a view with padding (`Same`/`Valid`), convolution at the type
  level.
- `buffer.rs` — `Buffer2D`/`Buffer4D` (nalgebra wrappers, no_std-compatible).
- `ops/` — one file per operator: `conv_2d`, `depthwise_conv_2d`,
  `fully_connected`, `average_pool_2d`, `reshape`, `softmax`, `transpose`.
- `activation.rs`, `quantize.rs` — fused activations and requant helpers.

The `microflow-macros/src/` macro crate:

- `lib.rs` — the `#[model(path)]` entry point: reads the `.tflite`, parses
  flatbuffers, generates `predict()` / `predict_quantized()` /
  `predict_inner()`.
- `ops/*.rs` — operator parsers: `Operator` → layer tokens
  (`Box<dyn ToTokens>`).
- `tensor.rs`/`buffer.rs` — tokenized versions of tensors/buffers (the
  weights are baked into the generated code as `const`s).
- `../flatbuffers/tflite_generated.rs` — the generated flatbuffers reader
  for the TFLite schema.

## The model path: `.tflite` → `predict()`

1. `#[model("models/sine.tflite")]` on a struct — the proc-macro, **at
   compile time**, reads the file and parses flatbuffers (`root_as_model`).
2. From subgraph 0 it takes: the input/output (shape, type,
   scale/zero-point), tensors, buffers (weights), the operator list.
3. Each operator → its own parser → layer tokens like:

   ```rust
   const filters_0: Tensor4D<i8, F, H, W, C, Q> = /* weights from the buffer */;
   let input: Tensor4D<_, OH, OW, OC, 1> = microflow::ops::conv_2d(
       input, &filters_0, [out_scale], [out_zero_point],
       Conv2DOptions { fused_activation, view_padding, strides },
       (const_0, const_1),  // requant constants, computed in the macro
   );
   ```

4. The layers are chained into `predict_inner()` — no allocations,
   everything in consts/on the stack.
5. The public contract:

   - `predict(buffer: Buffer2D/4D<f32, ...>) -> Buffer2D/4D<f32, ...>` —
     quantizes the input, dequantizes the output;
   - `predict_quantized(buffer: Buffer2D/4D<i8|u8, ...>) -> Buffer...<f32>` —
     the input is already quantized.

6. The macro writes the expanded code to `target/microflow-expansion.rs` —
   handy for debugging codegen.

## What is supported and what is missing for Conv1D

Supported operators (runtime + parser): `FULLY_CONNECTED`,
`DEPTHWISE_CONV_2D`, `CONV_2D`, `AVERAGE_POOL_2D`, `SOFTMAX`, `RESHAPE`,
`TRANSPOSE`. Types: int8/u8. Input/output ranks: **2 and 4** (rank-1 is
silently expanded to 2 by adding a leading 1).

Gaps for `Conv1D` (Keras `Conv1D` → tflite):

1. **Rank-3 input** `(1, T, C)` — the macro aborts: "supported ranks are 2
   and 4".
2. **RESHAPE with a rank-3 output/input** — the reshape parser aborts on
   ranks != 2/4; the `Reshape → CONV_2D → Reshape` chain needs rank 3 at
   the midpoint.
3. **Convolution with H=1 (a 1×k kernel)** — `conv_2d` will formally support
   it (it is 4D with stride 1 along one axis), but an efficient 1D kernel
   is needed separately (an int8 dot along the time axis, without nested
   loops over the dummy axis).
4. AVERAGE_POOL_2D for the 1D case — the same dummy-axis story.

The serialization fact (the D3 dump) — in
[`spike/conv1d-serialization.md`](../spike/conv1d-serialization.md);
the implementation contract — in [`docs/conv1d-spec.md`](./docs/conv1d-spec.md).

## Week 2 — the conv_1d kernel (facts for week 3)

- Kernel: `microflow/src/ops/conv_1d.rs` (registered in `ops/mod.rs`).
- **A deliberate difference from conv_2d**: the requant constants are NOT
  computed in the macro — the kernel takes raw scales (input / per-channel
  filters / output) and computes `m(f) = (scale_x·scale_w[f])/scale_out`
  itself. Week-3 codegen only needs to pass through the tensors' scale/zp
  and the int32 bias in acc units (spec §3.1/§3.3).
- **The bias is an explicit parameter** `Tensor2D<i32, F, 1, F>` in acc
  units (value = round(bias_real / (scale_x·scale_w[f]))), not baked into
  constants.
- **Rounding**: `f32::round_ties_even` (the spec), not `roundf` as in
  conv_2d — implementations may differ by ±1 quantum; the §5.3 tolerance
  covers that.
- **Saturation**: `T::from_superset_unchecked` in simba = the Rust `as`-cast
  (truncation + saturation) — confirmed by golden tests on saturating
  cases.
- **Same padding**: not via `TensorView` (which zero-fills + applies zp
  corrections in conv_2d), but with explicit geometry: windows reaching
  outside the bounds contribute nothing — which is exactly the "padding
  with the zp_x value" from spec §3.2. The TFLite formula:
  `out = ceil(T/stride)`, `pad_left = pad_total/2` (the extra one goes
  to the right).
- Golden: `tests/golden/conv1d.txt` — 96 cases (12 shapes × 8 variants,
  seed 42), the generator `examples/golden_gen.rs` (a naive reference
  inside), the test `tests/conv1d_golden.rs` — bit-for-bit; the shape
  dispatch table must match the generator.
- The dev dependency `rand = "0.9"` was added (the fixture generator); the
  runtime stayed no_std with no new dependencies.

## Week 3 — the parser/codegen and the FC extension (facts for week 4)

- **Shape folding (§2.1)** lives in `microflow-macros/src/shape_fold.rs`: a
  tensor → virtual shape table; `EXPAND_DIMS`/`RESHAPE`/`SHAPE`/
  `STRIDED_SLICE`/`PACK` produce no code. The spike graph folds 18 ops →
  6 layers (a unit test pins this on the real `models/conv1d.tflite`).
- **Rank-3 input (§2.2)**: the user-facing type is `Buffer2D<f32, T, C>`;
  internally the generated `predict` re-labels it into
  `Tensor4D<_, 1, 1, T, C, 1>` via `SMatrix::from_fn` (a stack copy, no
  heap — not literally zero-copy, but allocation-free).
- **Conversions**: when two layers' effective shapes differ (a folded
  RESHAPE), the driver emits `microflow::ops::reshape` bindings; 4D→4D hops
  through the flattened 2D form. 2D targets must be batch-major `(1, n)`.
- **The 1-D discriminator**: `CONV_2D` goes to `conv_1d` only when BOTH the
  filters height is 1 AND the effective input height is 1 (person_detect has
  1×1 filters over 96-row images — a real 2-D convolution).
- **FC per-channel (§3.3)**: the runtime takes a generic `WEIGHTS_QUANTS` and
  a 3-tuple of per-output constants (bias term, multiplier,
  zp correction); the old 4-tuple's scalar `INPUT_COLS*zp_x*zp_w` term merged
  into the per-column correction. A converter-dropped bias (F6) is
  synthesized as zeros by the macro.
- **conv_1d codegen**: scales/zps and the int32 bias are passed through
  raw; the bias is converted to accumulator units with
  `round(b_raw * scale_b / (scale_x * scale_w[f]))` — an identity when
  TFLite's `scale_b = scale_x*scale_w[f]` holds, a correction otherwise.
- **Quantization facts confirmed in the file**: conv filters carry 8 scales
  (per-channel, F5), FC weights 4 scales (per-channel, F6), activations
  per-tensor. The old reshape.rs parser is gone (folded);
  `ops/mod.rs` registers `conv_1d`.
- **Path conventions**: `#[model("...")]` resolves relative to the rustc
  cwd — the workspace root for workspace builds, the package dir for the
  fork's own targets. Test binaries run with the package dir as cwd.
- **Toolchain trap (fixed)**: `f32::round_ties_even`/`round`/`floor`/`trunc`
  are std-only in this no_std core build — the week-2 kernel silently relied
  on cached artifacts. `conv_1d.rs` now uses a `libm::truncf`-based
  `round_ties_even` helper (bit-identical, pinned by the golden fixtures).
  Rule: `cargo clean -p <crate>` before trusting a green cache after
  toolchain changes.
- **The bridge**: `nodes` depends on this fork by path; the OEE root
  `Cargo.toml` duplicates `[patch.crates-io] nalgebra` (a dependency's
  `[patch]` is ignored) and `exclude = ["fork"]` keeps the two workspace
  roots apart.

## Rust-ML track — the writer's own conventions and facts

The track (`tmp/docs/decompose/rust-ml.rus.md`) writes `.tflite` files from
Rust (burn training → own PTQ → own flatbuffers writer). The fork is NOT
modified: everything below lives in `ml/exporter` + `ml/trainer`.

- The generated flatbuffers bindings are NOT copied into the exporter: the
  crate includes the fork's `microflow-macros/flatbuffers/
  tflite_generated.rs` by `#[path]` (the same file the `#[model]` macro
  compiles against) — one source of truth, zero duplication; a fork schema
  update reaches the writer automatically.

Writer conventions (what `#[model]` accepts from a rust-born file):

- **Minimal graph**: exactly the 6 real operators, no EXPAND_DIMS/RESHAPE/
  Flatten wrappers. The rank-3 input `(1, 128, 1)` normalizes in the parser
  (§2.2), the rank-4 FC input `(1,1,30,16)` unfolds to `(1, 480)` (§2.1) —
  the folded path and the minimal path are the same code, a live test of
  the parser's generality (`ml/exporter/tests/model_microflow.rs`).
- `zero_point` is an **i64** vector in the schema (i64 in the writer,
  `i64::to_subset_unchecked` in the reader); scales are f32, per-channel
  for conv/FC weights (F entries, zp = 0 — F5/F6), per-tensor for
  activations (F7).
- Buffer 0 is empty (the TF convention); activation tensors point at it.
- `operator_codes` = `[CONV_2D, AVERAGE_POOL_2D, FULLY_CONNECTED, SOFTMAX]`,
  `deprecated_builtin_code` mirrors `builtin_code` (all fit in i8),
  `ModelArgs { version: 3 }`.
- Bias int32 per-channel with `scale_b = scale_x * scale_w[f]` — the
  macro's accumulator-unit conversion is then an identity.
- The softmax **output** (the model output tensor) gets `scale = 1/256,
  zp = -128` — the TF convention the kernel quantizes probabilities with.
  (The track plan's wording "softmax input" maps to this tensor; in the
  TF-converted file the 1/256 scale sits on `#32`, the softmax output.)
- Pools keep the input's quantization (the requant ratio is 1) — like the
  TF file, where pools share the convolution's scale.
- Asymmetric activation params use the FULL int8 range:
  `scale = (max-min)/255`, `zp = -128 - round(min/scale)` → `q(min) = -128`.
  (A naive `zp = round(-min/scale)` is the u8 formula — with it, zp clamps
  at 127, the top of the range saturates and logits collapse to equal
  quanta: softmax [0.5, 0.5, 0, 0]. Pinned by a quant unit test.)

Kernel facts confirmed from the rust-born side:

- **The softmax kernel does NOT subtract the max**: it computes
  `expf(logit * scale)` directly. With a unit-scale dummy (logits up to
  ±127, scale 1) `expf` overflows f32 → NaN → `as i8` = 0, and the output
  degenerates. Realistic PTQ scales keep `logit * scale` ≈ O(1) — safe.
  Dummies must go through PTQ (see `gen_dummy`), not unit scales.
- Pool/FC kernels round half-away-from-zero (`roundf`); conv_1d rounds
  ties-even — the interp reference (ties-even, float accumulator,
  per-layer requant) diverges from the kernels by ≤ 2 quanta per layer
  output, observed 0–1 on the fixtures (the D3.3 agreement, pinned by
  `ml/exporter/tests/model_a_parity.rs`).
- `predict_quantized`'s softmax ignores the input zp — shift-invariance of
  softmax; any zp on the FC output is fine.

Export seams (burn 0.21 → TFLite, pinned by trainer tests):

- burn `Conv1d` weights are `[F, C, k]` (cross-correlation, channel-first
  `[B, C, L]` input) → the file's OHWI `[F, 1, k, C]` is a swap of the last
  two axes.
- **burn 0.21 `Linear` stores weights as `[d_input, d_output]`** (`O = IW`),
  NOT the PyTorch `[Out, In]` — the export must transpose. Missing this
  silently transposes the FC and degrades int8 accuracy to ~0.25.
- The burn forward transposes `[B, C, L] → [B, L, C]` before flatten, so
  the FC input order is the TFLite row-major `(T, F)` — then the FC weights
  need no permutation at export.
- Split port (D4.3): per-class shuffle with `StdRng` seed 2026 and the py
  script's exact `int(len * (1.0 - 0.15))` cut — the class profiles match
  the Python track bit-exactly; the window membership differs (numpy's
  PCG64 shuffle is not reproduced — documented deviation, both tracks stay
  individually deterministic).
- Determinism: the full train→PTQ→write pipeline re-runs bit-identically
  (sha256 pinned in `ml/models/model_a.metrics.txt`); no thread pinning was
  needed for the NdArray backend.
- `#[model]` bakes the `.tflite` at COMPILE time — regenerating the model
  does not retrigger cargo. After a pipeline run: `touch` the test sources
  (`ml/exporter/tests/ml_metrics.rs`, `model_a_parity.rs`, `nodes/src/a.rs`)
  before re-running the microflow-side tests (see `ml/README.md`).

## Week 4 — nodes A/Q: no fork changes, two cross-tree facts

- The fork is untouched this week; `#[model]` baked the new
  `ml/models/model_q.tflite` (input `[1,1024,1]`, 6 real ops — the same
  minimal rust-born graph family as model_a) with no parser/codegen changes.
- The runtime model-path convention extends to test binaries (see the week-3
  note): `#[model]` resolves at compile time from the workspace root, but
  runtime readers run with the package dir as cwd — `nodes` integration
  tests load `../ml/models/model_q.tflite` through the exporter interp.
- MQTT deviates from the plan's `rumqttc`: an own minimal client (`mqtt-min`,
  std-only, QoS 0) because the offline sandbox has neither the crate nor a
  broker; see `docs/eng/week4-gate.md`, Deviations. The aggregator (week 5)
  will add SUBSCRIBE to the same crate.
