# Spike: Keras Conv1D serialization to .tflite (D3, week 1)

Russian version: [conv1d-serialization.ru.md](conv1d-serialization.ru.md)

> Recording the fact for risk #2 (plan section 11). Source: TF 2.21.0, the
> full-integer int8 converter, model architecture from plan section 6. The
> plan's expectation (section 5): "Conv1D → Reshape → CONV_2D over
> (1, T, C)". The fact — below.

## TL;DR

The expectation held, with refinements that matter for the parser:

1. The model input is **rank-3** `(1, T, C)` = `(1, 128, 1)`; the fork's
   macro currently supports only ranks 2 and 4.
2. Each Conv1D block is `EXPAND_DIMS → CONV_2D → RESHAPE`, i.e. **not one
   Reshape before CONV_2D, but an expand/squeeze pair around it**.
   `EXPAND_DIMS` is not supported by the fork at all.
3. The AvgPool block is the same pattern:
   `EXPAND_DIMS → AVERAGE_POOL_2D → RESHAPE`.
4. Flatten before FC is a dynamic chain `SHAPE → STRIDED_SLICE → PACK →
   RESHAPE`: the shape is computed at runtime. For a compile-time parser
   everything is static and the chain folds into a single RESHAPE, but each
   individual op is unsupported by the fork.
5. CONV_2D and FC weights are **per-channel** quantized (F scales for F
   filters), activations are per-tensor. Fork runtime: conv_2d supports
   per-channel, fully_connected — **per-tensor only** (QUANTS=1) — gap #2.
6. FULLY_CONNECTED **without bias**: the converter drops the zero bias
   (untrained Dense) and sets an optional −1 input. The fork's parser
   unconditionally expects the bias as the third input and panics — gap #3.

## Model

`Conv1D(8, k=3) → ReLU → AvgPool(2) → Conv1D(16, k=3) → ReLU → AvgPool(2) →
Flatten → Dense(4) → Softmax`, input `(128, 1)`, full-int8.

Script: `ml/scripts/build_conv1d_model.py`. Artifacts:
`ml/models/conv1d.tflite` (9.5 KB), the full dump —
`ml/models/conv1d_ops.txt`; the raw flatbuffers dump — via the probe crate
`tmp/tflite-probe` (the fork's reader, without fork modifications).

## Actual operator graph

```text
T#0 input (1,128,1) int8, scale=0.01282, zp=1
op[0]  EXPAND_DIMS   (T#0, axis=-3)          → T#15 (1,1,128,1)
op[1]  CONV_2D       (T#15, W(8,1,3,1), b(8)) → T#16 (1,1,126,8)   + fused RELU
op[2]  RESHAPE       (T#16, [-1,126,8])      → T#17 (1,126,8)
op[3]  EXPAND_DIMS   (T#17, axis=-3)         → T#18 (1,1,126,8)
op[4]  AVERAGE_POOL_2D (T#18)                → T#19 (1,1,63,8)
op[5]  RESHAPE       (T#19, [-1,63,8])       → T#20 (1,63,8)
op[6]  EXPAND_DIMS   (T#20, axis=-3)         → T#21 (1,1,63,8)
op[7]  CONV_2D       (T#21, W(16,1,3,8), b(16)) → T#22 (1,1,61,16)  + fused RELU
op[8]  RESHAPE       (T#22, [-1,61,16])      → T#23 (1,61,16)
op[9]  EXPAND_DIMS   (T#23, axis=1)          → T#24 (1,1,61,16)
op[10] AVERAGE_POOL_2D (T#24)                → T#25 (1,1,30,16)
op[11] RESHAPE       (T#25, [-1,30,16])      → T#26 (1,30,16)
op[12] SHAPE         (T#26)                  → T#27 [3]
op[13] STRIDED_SLICE (T#27, [0], [1], [1])   → T#28
op[14] PACK          (T#28, 480)             → T#29 [2]
op[15] RESHAPE       (T#25, T#29)            → T#30 (1,480)
op[16] FULLY_CONNECTED (T#30, W(4,480), b=-1) → T#31 (1,4)
op[17] SOFTMAX       (T#31)                  → T#32 (1,4) int8, zp=-128
```

## Weights layout and quantization

| Tensor         | Shape          | Layout / quantization                                         |
| -------------- | -------------- | ------------------------------------------------------------- |
| Conv1D weights | `(F, 1, k, C)` | OHWI: (out, h=1, w=k, in); per-channel, F scales, zp=0        |
| Conv1D bias    | `(F,)` int32   | per-channel: scale_b = scale_in × scale_w[f]                  |
| FC weights     | `(Out, In)`    | (out, in); per-channel by default — the fork needs per-tensor |
| FC bias        | `(Out,)` int32 | the zero bias is dropped by the converter (−1 input)          |
| Activations    | any            | per-tensor, 1 scale                                           |

Layout check: in the fork's own `person_detect.tflite` model the CONV_2D
weights are `[16,1,1,8]` — the same OHWI. That is, the TF 2.21 layout
matches the fork parser's expectations; the only discrepancy is the tensor
rank (rank-3 at block inputs/outputs).

Sanity: .tflite output (interpreter) vs Keras — max|diff| = 0.0024 on the
softmax probabilities; argmax matches. The conversion is correct.

## Bonus: the full minimal Keras → tflite → Rust loop

`ml/scripts/build_dense_model.py` builds `Dense(16,relu) → Dense(4) →
Softmax` (input (1,8), full-int8, **per-tensor** weights, non-zero bias) →
`fork/microflow/models/dense_spike.tflite` → the example
`fork/microflow/examples/dense_spike.rs`:

```text
Probabilities: [0.1016, 0.1719, 0.2109, 0.5156]
Predicted class: 3
```

Two practical converter limitations found along the way (workarounds — in
the script):

- a zero FC bias gets dropped → for the spike the bias is initialized
  non-zero;
- per-channel FC weights are not supported by the fork runtime → disabled
  with the `_experimental_disable_per_channel` flag (for production node
  models this is handled by the Conv1D spec: per-channel is a mandatory
  part of the kernel).

## Takeaway for the spec (D4)

The parser's real scope is larger than "understand the Reshape chain": it
must fold the `EXPAND_DIMS`/`RESHAPE` wrappers, statically evaluate the
Flatten `SHAPE/STRIDED_SLICE/PACK` chain, support a rank-3 input,
per-channel weights (FC!) and an optional bias. The kernel, in turn, works
with an already-expanded 4D tensor (1,1,T,C) — as a regular CONV_2D with
h=1.
