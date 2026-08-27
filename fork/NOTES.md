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
