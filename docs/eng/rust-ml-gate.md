# Rust-ML track gate

> The stretch track ([decomposition](../../tmp/docs/decompose/rust-ml.rus.md)):
> `.tflite` for node A born entirely in Rust — `cargo run` does train (burn) →
> PTQ (own code) → export (own flatbuffers writer) → self-check. Zero Python
> in the repro loop.

Status: **implemented** (branch `feat/rust-ml`). The "main or parallel
path" decision (D7.3) is for the mentor review — `plan.md` section 6
is intentionally untouched until then.

## The gate checklist

- [x] `ml/models/model_a.tflite` generated entirely in Rust (one command),
      node A (`nodes/src/a.rs`) predicts on it.
      — pipeline: `cargo run -p trainer --release --bin train -- --datasets
      tmp/ds_*.csv --calib 256 --out ml/models/model_a.tflite`;
      `nodes` test `a::tests::model_builds_through_the_bridge` green.
- [x] A re-run is bit-identical (sha256 pinned).
      — `sha256: 99e719d3870ddbdccf9070c67065a4e478e0eec9f5e67de263860ceb0ce772b9`
      in `ml/models/model_a.metrics.txt`; double-run compared with `cmp`.
- [x] int8 val accuracy not worse than float minus 2%; the confusion matrix
      recorded in the py script's format.
      — float (burn) 1.0000, int8 1.0000 (drop 0%), `ml/models/model_a_metrics.txt`
      (microflow `#[model]`), `ml/models/model_a.metrics.txt` (pipeline).
- [x] Parity without TF: microflow vs the naive Rust reference — ±1–2 quanta
      on val; float (burn) vs int8 — argmax agreement, `max|Δp| ≤ 0.05`.
      — `model_a_parity.rs`: 64 windows, 0 argmax disagreements,
      max|Δp| = 0.0039; observed element diffs 0–1 quanta (budget 2).
- [x] `cargo test` green in the workspace; offline build of everything except
      burn training.
      — `cargo test --workspace --release`: all green (incl. the pre-existing
      suites); `cargo fmt`/`clippy -D warnings` clean. `exporter` builds
      offline; `trainer` needs one network fetch of burn 0.21.0.

## Artifacts

| Artifact                                   | Role                                              |
| ------------------------------------------ | ------------------------------------------------- |
| `ml/exporter`                              | writer + PTQ + interp + dumper + fixtures         |
| `ml/trainer`                               | burn model/data/loop + the pipeline CLI           |
| `ml/models/model_a.tflite` + `.ops.txt`    | the rust-born model + the structure dump          |
| `ml/models/model_a.float` / `.val.csv`     | float weights / the deterministic val split       |
| `ml/models/model_a_parity.txt`             | parity fixtures (interp expectations)             |
| `fork/NOTES.md` (Rust-ML section)          | the writer's conventions + kernel facts           |

## Python vs Rust model

|                        | Python (week 3)                | Rust (this track)                          |
| ---------------------- | ------------------------------ | ------------------------------------------ |
| train                  | Keras (TF venv)                | burn 0.21 (NdArray + autodiff), Adam       |
| quantize               | `tf.lite.TFLiteConverter`      | own PTQ (`ml/exporter/src/quant.rs`)       |
| export                 | the TF converter               | own flatbuffers writer (`writer.rs`)       |
| val split              | numpy PCG64, seed 2026         | `StdRng`, seed 2026 (same class profiles)  |
| int8 val accuracy      | (TF-venv dependent)            | 1.0000 (interp and microflow agree)        |
| model size             | 5064 bytes (same architecture) | 5064 bytes                                 |
| determinism            | seeds fixed, venv-dependent    | bit-identical re-runs (sha256 pinned)      |
| graph                  | 18 ops + wrappers              | 6 real ops, no wrappers                    |
| Python in the loop     | yes (venv, TF install)         | none                                       |

## Deviations from the plan (recorded)

- The per-class shuffle is ported in logic, not in numpy's PCG64 bits: window
  membership differs from the Python split, the class profiles match exactly
  (the cut formula is bit-identical). Both tracks are individually
  deterministic.
- The metrics-through-microflow step is a test target (`ml_metrics.rs`), not
  part of the one command: `#[model]` is compile-time and cargo does not
  rebuild on artifact change (the `touch` flow is in `ml/README.md`).
- burn pinned to 0.21.0 (the fresh stable at D0); features `ndarray`,
  `autodiff`, `std` — no Learner/dataset infra, the training loop is manual
  (per the anti-scope).

## Not done / future

- `tract` as an external parity reference (the `--tract` flag) — not needed:
  interp + float-parity cover the safety net; revisit if the kernel set grows.
- QAT (anti-scope), the TF-compatible wrapper graph (anti-scope).
