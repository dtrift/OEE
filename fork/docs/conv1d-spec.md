# Spec: Conv1D for microflow-rs (the week 2–3 contract)

> Written from the results of the D3 spike (facts —
> [`spike/conv1d-serialization.md`](../../spike/conv1d-serialization.md)).
> Readiness criterion: "a third-party developer implements from the spec
> without questions".
> If the serialization facts change (a different TF version) → update the
> spike doc first, then this spec.

## 1. Input data (facts from D3)

Model: Keras `Conv1D(8,k=3) → ReLU → AvgPool(2) → Conv1D(16,k=3) → ReLU →
AvgPool(2) → Flatten → Dense(4) → Softmax`, input `(128, 1)`, full-int8,
TF 2.21.

Facts that everything below rests on:

- F1. The subgraph input is rank-3 `(1, T, C)`.
- F2. A Conv1D block = `EXPAND_DIMS(axis=-3) → CONV_2D → RESHAPE([-1, T', F])`.
- F3. An AvgPool block = `EXPAND_DIMS → AVERAGE_POOL_2D → RESHAPE`.
- F4. Flatten = `SHAPE → STRIDED_SLICE → PACK → RESHAPE` (a dynamic shape;
  static for us).
- F5. CONV_2D weights are OHWI `(F, 1, k, C)`, per-channel (F scales, zp=0);
  the bias is `(F,)` int32, per-channel (scale_b = scale_in × scale_w[f]).
- F6. FC: weights `(Out, In)` per-channel (by default), the bias optional:
  a zero one is dropped by the converter (a −1 input).
- F7. Activations are per-tensor (1 scale); fused RELU in the CONV_2D
  options.

Target vertical: parser (microflow-macros) → codegen → kernel (runtime).

## 2. Parser (week 3, microflow-macros)

### 2.1 Graph normalization (before layer generation)

The graph goes through a "shape-folding" pass:

1. `EXPAND_DIMS(x, axis)` → a virtual reshape: the input shape + the axis.
2. `RESHAPE(x, [-1, a, b])` and `RESHAPE(x, [a, b, c])` → a virtual
   reshape; `-1` is computed from the product of the rest (an error if it
   does not divide evenly).
3. `SHAPE → STRIDED_SLICE → PACK → RESHAPE` (the Flatten chain) — computed
   statically: the `PACK` result is known from the shapes; the whole chain
   is replaced by one virtual reshape `(1, T'', F'') → (1, T''*F'')`.

Pass rules:

- The pass walks the operator list sequentially, keeping a "tensor →
  virtual shape" table. Shape operators produce no code.
- Virtual reshapes are applied to the next "real" operator (CONV_2D,
  AVERAGE_POOL_2D, FULLY_CONNECTED, SOFTMAX): its input gets the final
  shape from the table.
- If a reshape chain reduces the data to rank ≤ 2 before FC — that is the
  normal Flatten path (see F4).
- Anything that does not fold (e.g. PACK with unknown values) —
  `abort_call_site!` with a clear message: which op, which shape, what was
  expected.

### 2.2 Rank-3 macro input/output

- The input `(1, T, C)` is accepted and normalized to `(1, 1, T, C)`
  (rank-4, h=1). The user-facing API is `Buffer2D<f32, T, C>` (the data is
  the same; the representation shape is 2D, like the rank-3 tensor (1, T, C)
  after squeezing the batch axis).
- The rank-3 output `(1, K)` similarly → `Buffer2D<f32, 1, K>`.
- Rule: the batch axis (the first one, =1) does not enter the user type;
  internal representations are only rank-4 `(1, 1, T, C)` for convolutions
  and rank-2 for FC/softmax.

### 2.3 Operators

- `EXPAND_DIMS`: input + a scalar axis; shape only, does not touch the
  data.
- `RESHAPE`: the second input is a constant int32 vector (read from the
  tensor's buffer); the `new_shape` options are ignored if the input is
  set.
- `SHAPE`, `STRIDED_SLICE`, `PACK`: only inside the foldable Flatten chain;
  outside it — unsupported.
- `CONV_2D` with filters `(F, 1, k, C)`: validate `h == 1` — otherwise it
  is not the 1D case; hand it to the existing conv_2d path (without
  changing its logic).
- `FULLY_CONNECTED`: `inputs.len() == 2` → no bias (F6): a zero bias is
  passed to codegen (a constant vector of length Out). The weights layout
  `(Out, In)` — as now.
- `AVERAGE_POOL_2D` with a `(1, p)` filter: h=1 — the existing path with
  the 4D representation fits.

### 2.4 Quantization

- CONV_2D weights: per-channel is mandatory (F5) — already supported by the
  `TokenTensor4D` structure (QUANTS = F).
- FC weights: per-channel (F6) — either add QUANTS > 1 support to the
  `fully_connected` runtime (see §3.3), or pin per-tensor in the project's
  converter (the `_experimental_disable_per_channel` workaround). The
  week-3 decision: support per-channel in the runtime (the right way); in
  the ml scripts — a switchable flag.
- The int32 per-channel bias: scale_b[f] is read from the bias tensor
  directly.

## 3. Kernel (week 2, the `microflow::ops` runtime)

### 3.1 conv_1d semantics

Input: `Tensor4D<T, 1, 1, T, C, 1>` (h=1). Weights: OHWI `(F, 1, k, C)`,
per-channel. Output: `(1, 1, T_out, F)`.

```text
acc(i32) = Σ_{t,c} (x[t,c] - zp_x) * (w[f,0,t,c] - zp_w[f])   [dot over the k window]
raw(f, t) = acc + bias[f]                                       [bias already in acc units]
out(f, t) = saturate_i8( round_ties_even( raw(f,t) * m(f) ) + zp_out )
m(f) = (scale_x * scale_w[f]) / scale_out                        [per-channel multiplier]
```

- The accumulator is **i32** (overflow is impossible with T·C ≤ 2^16 and
| x−zp | ≤ 255: max 255·255·2^16 < 2^31 — check with asserts at codegen |
  time).
- Requant is a per-channel multiplier `m(f)`. The conversion to i8 is via
  `(acc as f32 * m(f)).round_ties_even() + zp_out`, then
  `clamp(-128, 127)`.
- Rounding: `round_ties_even` (banker's), pinned identically for the
  reference and the kernel (see §5).
- The fused activation (RELU from the CONV_2D options) is applied to the
  output BEFORE storing: relu(x) = max(x, zp_out) in quantized coordinates.

### 3.2 Geometry

- `stride` is along the time axis (w); `padding`: `valid` — windows from 0;
  `same` — symmetric padding, the output is `ceil(T/stride)`, padding with
  zeros IN QUANTIZED COORDINATES: the value is zp_x (not 0!).
- `T < k`: valid → an empty output (a codegen-time error: an output with a
  zero axis is forbidden); same → an output of length ceil(T/stride), the
  windows padded with zp_x.
- `T_out(valid) = floor((T - k)/stride) + 1` — with `T < k` and stride > 1
  the formulas must not produce 0 without a check.

### 3.3 The fully_connected extension (per-channel)

The runtime signature changes from `QUANTS=1` to a generic `QUANTS` (as in
conv_2d): the requant constants are computed per-channel in the macro's
preprocessing. The bias is optional (a zero constant when absent) — §2.3.

## 4. Codegen (week 3, microflow-macros)

1. After the §2.1 normalization the graph consists of: CONV_2D(h=1),
   AVERAGE_POOL_2D(h=1), FULLY_CONNECTED, SOFTMAX — generation as now,
   plus:
2. A rank-3 model input → `predict()` takes `Buffer2D<f32, T, C>` and
   internally expands it to `(1,1,T,C)` (zero-copy: the same data, a
   different logical shape).
3. A rank-3 output `(1, K)` → the same squeeze into `Buffer2D<f32, 1, K>`.
4. For FC without a bias: a constant zero bias vector of length Out
   (int32, zp=0).
5. Codegen asserts: `h == 1` for convolutions/pools; `T ≥ k` for valid;
   buffer sizes match the product of the shapes.
6. `target/microflow-expansion.rs` is still written — we test codegen with
   it.

## 5. Tests (weeks 2–3)

### 5.1 Toy test (by hand, D1 of week 2)

1 channel, kernel 3, stride 1, valid, T=5; the input/weights/output are
computed by hand (including zp and scale) — a bit-for-bit comparison.

### 5.2 Golden tests (a Rust reference, D3 of week 2)

- The reference implementation: naive Rust per the §3.1 formulas (no
  optimizations, an i32 accumulator, `round_ties_even`); it lives in the
  test infrastructure.
- The `golden-gen` generator (a bin in the fork): writes fixtures —
  human-readable files (input, weights, bias, quant parameters, the
  expected output).
- Cases: T 1–64, channels 1–8, kernel 1–7, stride 1–2, valid/same — ~100
  of them; a deterministic seed.
- The comparison is **bit-for-bit** (no tolerance: the same integer
  semantics).
- A mismatch → a bug on one of the sides; investigate per operation, do
  not pick a tolerance.

### 5.3 The week-3 safety net (a cross-check against TFLite)

Host inference of the real `conv1d.tflite` via `#[model]` against the
TFLite interpreter output (Python): a tolerance of ±1 quantum at the output
(a different order of operations is allowed between requant
implementations — record the fact).

## 6. Definition of Done

### Week 2 (kernel)

- [ ] conv_1d passes the toy test (§5.1) and all golden cases (§5.2).
- [ ] Edge cases: `T < k` (same), `stride=2`, `valid/same`, 8 channels.
- [ ] `cargo test` + `cargo clippy --all-targets -- -D warnings` green.
- [ ] The kernel is `no_std`-compatible (no allocations, no std crates).

### Week 3 (parser + codegen)

- [ ] `#[model]` accepts a model with a rank-3 input (§2.2).
- [ ] The §2.1 shape-folding folds the whole spike graph (18 ops → 6
  layers).
- [ ] FC without a bias and with per-channel weights passes §5.3 (±1
  tolerance).
- [ ] Host inference of `conv1d.tflite` via `#[model]` matches the TFLite
  interpreter within ±1 quantum on all test windows.
- [ ] `cargo test` + clippy green; the `conv1d_spike` example in the fork.
