# Week 3 — decomposition (parser + codegen + ML pipeline)

> Branch: `feat/parser-codegen-ml` (OEE) + `feat/conv1d-parser-codegen` (fork/microflow)

> Breakdown of the "Week 3" row of the plan [`plan.md`](../plan.md), plan section 9. Two
> tracks: (1) `Conv1D` in `microflow-macros` — a shape-folding parser + codegen, so that
> a real `.tflite` builds through `#[model]`; (2) training the node A model on synthetic
> data + parity. Mode: 1 person; weekdays ~2–3 h, Saturday ~4 h. Estimate: ~16–19 h.
>
> Input: the week 2 gate — the `conv_1d` kernel is green (toy + golden), spec §2/§4
> awaits implementation. A week 1 fact: the real graph is not a "Reshape chain" but
> `EXPAND_DIMS → CONV_2D → RESHAPE` + a dynamic Flatten (`SHAPE → STRIDED_SLICE → PACK →
> RESHAPE`); spec §2.1 covers this, the goal is 18 op → 6 layers (DoD §6).
>
> Detailing of the draft [`plan/step-3.md`](../plan/step-3.md): tied to the spec and the
> artifacts of weeks 1–2; on conflict → this file is edited, the plan only in substance.

## Week gate (minimum done)

- [ ] `ml/models/conv1d.tflite` builds through `#[model]`, `predict()` on the host —
      the `conv1d_spike` example in the fork (DoD §6).
- [ ] Parity: `#[model]` and tf.lite.Interpreter — the same predictions, tolerance ±1
      quant (§5.3).
- [ ] The node A model trained on synthetic data, full-int int8, accuracy recorded.
- [ ] The feature golden test: Rust (`features-cli`) vs numpy — feature parity (plan
      section 6).

## Day-by-day summary

| Day | Session topic | Essence                                    | Artifact                    |
| --- | ------------- | ------------------------------------------ | --------------------------- |
| D0  | engine        | branches, spec §2/§4, §3.3 status          | questions resolved pre-code |
| D1  | engine        | shape-folding parser (§2.1–2.2)            | 18 op → 6 layers in a test  |
| D2  | engine        | operators (§2.3–2.4) + codegen (§4)        | the spike model builds      |
| D3  | engine        | real `.tflite` through `#[model]` + parity | gate item closed            |
| D4  | ML            | `features-cli` features + node A dataset   | train/val dataset           |
| D5  | ML            | training model A + int8                    | `model_a.tflite` + metrics  |
| D6  | ML            | model A parity + workspace → fork bridge   | parity green, path-dep OK   |
| D7  | gate          | checklist, retro, week 4 plan              | `docs/week3-gate.md`        |

## D0 (evening before start, ~0.5 h): branches and spec

1. Branches: `feat/conv1d-parser-codegen` in `fork/microflow`, `feat/parser-codegen-ml`
   in OEE.
2. Re-read spec §2 (parser), §4 (codegen); keep at hand the operator dump
   `ml/models/conv1d_ops.txt` and [`spike/conv1d-serialization.md`](../../../spike/conv1d-serialization.md).
3. Check the §3.3 status (FC per-channel + optional bias): if week 2's D4 buffer did
   not close it — account for it in D2 (codegen must support QUANTS > 1 on FC).

## D1 (Mon, ~3 h) — engine: shape-folding parser

1. The normalization pass §2.1: a "tensor → virtual shape" table; `EXPAND_DIMS`/
   `RESHAPE` produce no code; the Flatten chain `SHAPE → STRIDED_SLICE → PACK →
   RESHAPE` is computed statically and folded into a single virtual reshape.
2. Rank-3 input/output §2.2: `(1, T, C)` is normalized to `(1, 1, T, C)`.
3. Parser unit tests on the actual spike graph: put `conv1d.tflite` into
   `fork/microflow/models/` (following the `dense_spike.tflite` precedent).
4. Errors: anything not foldable — `abort_call_site!` with a "which op, which shape,
   what we expect" message (§2.1), not a context-free panic.

Check: folding the spike graph yields exactly 6 layers — CONV_2D, AVERAGE_POOL_2D,
CONV_2D, AVERAGE_POOL_2D, FULLY_CONNECTED, SOFTMAX.

## D2 (Tue, ~3 h) — engine: operators and codegen

1. Operators §2.3: `CONV_2D` with an `h == 1` validation (otherwise — the existing
   conv_2d path), `AVERAGE_POOL_2D` with an `(1, p)` filter, `FULLY_CONNECTED` with
   `inputs.len() == 2` → a constant zero bias (F6), `SOFTMAX`.
2. Quantization §2.4: per-channel for CONV_2D already exists (`TokenTensor4D`, QUANTS =
   F); FC — QUANTS > 1 if §3.3 is not done (see D0), otherwise per-tensor via a flag in
   the ml script — record the decision in one line in NOTES.
3. Codegen §4: rank-3 input → `predict()` accepts `Buffer2D<f32, T, C>` (zero copy);
   asserts `h == 1`, `T ≥ k` (valid), buffer sizes; `target/microflow-expansion.rs` —
   for reviewing the codegen.
4. Compile-fail tests: a wrong model → a readable compile error.

Check: the (untrained) spike model builds through `#[model]`; the expansion reads well
and is understandable.

## D3 (Wed, ~3 h) — engine: real `.tflite` through `#[model]` [gate]

1. `ml/models/conv1d.tflite` → `#[model]` → `predict()` on the host — the
   `conv1d_spike` example in the fork (modeled on `examples/dense_spike.rs`).
2. Parity §5.3: the same inputs into tf.lite.Interpreter (Python 3.12, `tmp/venv312`)
   and into Rust — tolerance ±1 quant (the requant operation order may differ — record
   the fact).
3. Typical mismatches: OHWI weight layout, `zp_x` padding, rounding — cross-check
   against spec §3.

Check: the parity test is green — the week's main gate item is closed.

## D4 (Thu, ~2–3 h) — ML: features and the node A dataset

1. Model A consumes raw windows `(128, 1)` — like the spike model (`WindowSpec(A)` =
   128 @ 1.6 kHz, `features-cli/src/lib.rs`). Features (RMS, peak, zero-crossings,
   spectrum) — not a model input but a dataset analysis tool and a parity safety net:
   fix the list in `features-cli`.
2. Dataset: `line-simulator` (the week 2 D6 scenarios: normal/downtime/degradation,
   different seeds) → CSV → windows per `WindowSpec(A)` + labels from ground truth (the
   `state` column); export for Python (fix the windows+labels format).
3. Golden features: one fixed window → Rust and numpy → integer features bit-for-bit,
   float — ±1e-6 (record the fact).

Check: the dataset is balanced across classes; golden features green.

## D5 (Fri, ~3 h) — ML: training and int8

1. `ml/scripts/train_model_a.py` modeled on `build_conv1d_model.py`: the plan section 6
   architecture, input `(128, 1)`, seed fixed — the script is reproducible.
2. Training; full-integer int8 PTQ; export `ml/models/model_a.tflite`; operator dump —
   the structure must match the spike graph (the same converter).
3. Accuracy on val (a rough confusion matrix); if ~100% — raise the simulator noise
   (the plan section 11 risk) and rebuild the dataset.
4. Week 1 converter quirks: a zero FC bias is dropped; per-channel FC — if §3.3 is not
   done, the `_experimental_disable_per_channel` flag (a workaround in the script).

Check: metrics recorded; re-running the script yields the same model.

## D6 (Sat, ~3 h) — ML: parity closed + bridge into the workspace

1. `model_a.tflite` → `#[model]` → predictions on val match the interpreter (±1 quant).
2. The week 4 bridge: wire up the path dependency `nodes → fork/microflow` and
   uncomment `[patch.crates-io] nalgebra` in the root `Cargo.toml` (a quirk from
   [`fork/NOTES.md`](../../../fork/NOTES.md), D1); `cargo test` green in both
   workspaces.
3. A rough confusion matrix for A (plan section 10).

Check: parity green. Parity fails → a week 4 blocker, fix it now.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; every "yes" — backed by a link to an artifact.
2. Retro: hours vs estimate; what slowed things down.
3. Week 4 plan: nodes A and Q end-to-end + the cut-line at the end of the week.

Artifact: `docs/week3-gate.md` — checklist + 5–10 lines of conclusions (in the early
plan — `tmp/OEE/week3-gate.md`; `tmp/` is gitignored, precedent — week 1).

## Escalation points

- The parser cannot handle the actual graph (outside §2.1) → fix the spec first, then
  the code; if the graph changed with a TF version — regenerate the
  `build_conv1d_model.py` dump, update the spike doc and the spec.
- Parity diverges by more than ±1 quant → investigate layer by layer: the interpreter's
  intermediate tensors against the expansion code; first the CONV_2D blocks, then FC.
- int8 accuracy dropped >2% vs float → record it in the report; QAT — future work (plan
  section 12), do not get distracted now.
- Training is poor on raw windows → one experiment: features as channels (C > 1);
  remember: the `(128, 1)` input is already supported by the parser, C > 1 is an
  extension.
- FC per-channel not implemented (§3.3) → per-tensor via a flag in the train script; the
  fact — into NOTES and the report.

## Anti-scope (what we do NOT do in week 3)

- Nodes, MQTT, dashboard — weeks 4–5 (the path-dependency bridge is D6, not
  integration).
- The node Q model (sound) — week 4 (synthesis not ready yet).
- QEMU and benchmarks — week 6; an upstream PR — after the course (plan section 12).
