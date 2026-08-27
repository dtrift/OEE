# Week 3 — decomposition (parser + codegen + ML pipeline)

> Branch: `feat/parser-codegen-ml` (OEE) + `feat/conv1d-parser-codegen` (fork/microflow)

> Decomposition of the "Week 3" row from [`plan.md`](../plan.md),
> section 9. Two lines: (1) `Conv1D` in `microflow-macros` — a flatbuffers parser + codegen,
> so that a real `.tflite` builds through `#[model]`; (2) training the node A model on
> synthetic data, int8 quantization, parity tests. Mode: 1 person; weekdays ~2–3 h,
> Saturday ~4 h. Estimate: ~16–19 h. Entry: the kernel is green, the spec, the current signal is generated.

## Week gate (minimum ready)

- [ ] A real `.tflite` with `Conv1D` builds through `#[model]` and `predict()` on the host.
- [ ] Parity: `#[model]` and tf.lite.Interpreter give the same predictions on the same inputs.
- [ ] The node A model is trained on synthetic data, full-int int8, the accuracy is recorded.
- [ ] Feature golden tests: Rust (`features-cli`) vs numpy — feature parity (section 6).

## Day-by-day summary

| Day | Session topic | Summary                                      | Artifact                        |
| --- | ------------- | -------------------------------------------- | ------------------------------- |
| D1  | engine        | parser: 3D tensors + the `Reshape` chain     | the parser reads the spike dump |
| D2  | engine        | codegen: the kernel call from `#[model]`     | the spike model builds          |
| D3  | engine        | real `.tflite` through `#[model]` + parity   | the gate item is closed         |
| D4  | ML            | node A features + dataset from the simulator | train/val dataset               |
| D5  | ML            | Conv1D model training + int8 quantization    | `model_a.tflite` + metrics      |
| D6  | ML            | model parity + feature golden                | the parity test is green        |
| D7  | gate          | checklist, retro, the week 4 plan            | `tmp/OEE/week3-gate.md`         |

## D1 (Mon, ~3 h) — engine: the flatbuffers parser

1. Per the spec: reading `CONV_2D` over `(1, T, C)`, folding the `Reshape` chain,
   the stride/padding attributes, 3D tensor shapes.
2. Parser unit tests on the dumps from the week 1 spike (the actual graph).
3. Understandable errors: an unsupported attribute → a comprehensible message, not a panic.

Check: the parser parses all the spike dumps without panics.

## D2 (Tue, ~3 h) — engine: codegen

1. The generated code: buffers for weights/outputs, a call to the week 2 kernel.
2. The model from the spike (untrained) builds through `#[model]`.
3. Compile-fail tests: a wrong model → a readable compilation error.

Check: `cargo expand` — the generated code reads well and is understandable.

## D3 (Wed, ~3 h) — engine: a real `.tflite` through `#[model]` [gate]

1. The `.tflite` from the spike → `#[model]` → `predict()` on the host.
2. Parity: the same inputs into tf.lite.Interpreter (Python) and into Rust — the same outputs
   (within the quantized tolerance).
3. Typical mismatches: the weight layout, the requant order — cross-check against the spec.

Check: the parity test is green — the week's main gate item is closed.

## D4 (Thu, ~2–3 h) — ML: node A features and dataset

1. `features-cli`: the set of current-window features (RMS, peak, zero-crossings, spectrum) —
   fix the list; the same crate later runs in inference (section 6).
2. Dataset: `line-simulator` → CSV → features → train/val; feature export for Python.
3. The feature golden test: Rust vs numpy on the same window — a discrepancy of 0.

Check: the dataset is balanced across classes; the golden features are green.

## D5 (Fri, ~3 h) — ML: training and int8

1. Model A per section 6: `Conv1D(8)→AvgPool→Conv1D(16)→AvgPool→FC→Softmax`.
2. Training; full-integer int8 (post-training), export `.tflite`.
3. Accuracy on val; if ~100% → raise the noise in the simulator (the section 11 risk)
   and rebuild the dataset.

Check: the metrics are recorded; the training script is reproducible by seed.

## D6 (Sat, ~3 h) — ML: the parity loop is closed

1. `model_a.tflite` → `#[model]` on the host: the predictions on val match
   the interpreter.
2. A draft confusion matrix for A (section 10) — from a run, without polish.
3. Parity fails → this is a blocker for week 4, fix it now.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; links to artifacts.
2. Retro: hours, bottlenecks.
3. The week 4 plan: nodes A and Q end-to-end + the cut-line at the end of the week.

Artifact: `tmp/OEE/week3-gate.md`.

## Escalation points

- The parser can't handle the `Reshape` chain → simplify the input: regenerate the `.tflite`
  with a script using a different input shape; the fact and the decision — into `NOTES.md` and the report.
- int8 accuracy dropped >2% vs float → record it in the report; QAT — future
  work (section 12), don't get distracted now.
- Training goes badly on features → try raw windows as the input (Conv1D extracts
  the features itself) — one decision per one experiment, not ten.

## Anti-scope (what we do NOT do in week 3)

- Nodes, MQTT, the dashboard — weeks 4–5.
- The node Q model (sound) — week 4 (the synthesis isn't ready yet).
- QEMU and benchmarks — week 6; an upstream PR — after the course (section 12).
