# ml/ — the ML pipeline of the OEE project

Two paths live side by side:

- **Python (week 3, legacy)**: `ml/scripts/*` — Keras training, the TF
  converter, `tf.lite.Interpreter` metrics. Needs the TF venv. The source of
  the serialization facts F1–F7 (`fork/docs/conv1d-spec.md`).
- **Rust (the rust-ml track)**: `ml/exporter` + `ml/trainer` — burn training,
  own PTQ, own flatbuffers writer, own float reference. Zero Python in the
  repro loop; a re-run is bit-identical. See
  [tmp/docs/decompose/rust-ml.rus.md](../tmp/docs/decompose/rust-ml.rus.md)
  and the facts in [fork/NOTES.md](../fork/NOTES.md).

Since week 4 the trainer is task-parameterized: `--task a` (default) trains
node A on `--dataset` CSVs, `--task q` trains node Q on `--taps-dataset`
CSVs (`line-simulator --taps-dataset`). One `ModelCnn`, runtime dims
(`trainer::TaskSpec`).

## The one-command pipeline (rust track)

```bash
# node A (current windows):
cargo run -p trainer --release --bin train -- \
    --datasets tmp/ds_base_1.csv ... tmp/ds_jam_42.csv \
    --calib 256 --out ml/models/model_a.tflite

# node Q (tap windows, week 4):
cargo run -p line-simulator -- --scenario scenarios/taps.toml --seed N \
    --taps-dataset tmp/dsq_N.csv --taps-meta tmp/dsq_N_meta.csv
cargo run -p trainer --release --bin train -- --task q \
    --datasets tmp/dsq_*.csv --calib 256   # -> ml/models/model_q.tflite
```

Artifacts (in `ml/models/`):

| File                  | What it is                                            |
| --------------------- | ----------------------------------------------------- |
| `model_a.tflite`      | the int8 model, born entirely in Rust                 |
| `model_a.ops.txt`     | the operator dump (diffable against `conv1d_ops.txt`) |
| `model_a.float`       | float weights (JSON header + f32 LE)                  |
| `model_a.val.csv`     | the deterministic val split (`label,x000..x127`)      |
| `model_a.metrics.txt` | pipeline metrics (interp side)                        |
| `model_a_metrics.txt` | microflow `#[model]` metrics (see below)              |
| `model_a_parity.txt`  | parity fixtures (interp expectations)                 |
| `model_q.tflite`      | node Q int8 model (week 4, same pipeline, `--task q`) |
| `model_q.ops.txt`     | node Q operator dump                                  |
| `model_q.float`       | node Q float weights                                  |
| `model_q.val.csv`     | node Q val split (`label,x000..x1023`)                |
| `model_q.metrics.txt` | node Q pipeline metrics + sha256                      |

## The microflow-side checks (after the pipeline)

`#[model]` bakes the `.tflite` **at compile time** — regenerating the model
does not retrigger cargo. After a pipeline run:

```bash
cargo run -p exporter --release --bin parity_gen     # refresh the fixtures
touch ml/exporter/tests/ml_metrics.rs ml/exporter/tests/model_a_parity.rs nodes/src/a.rs
cargo test -p exporter --release --test ml_metrics -- --nocapture
cargo test -p exporter --release --test model_a_parity
cargo test -p nodes --release
```

## The exporter toolbox

```bash
cargo run -p exporter --bin dump_model -- ml/models/model_a.tflite  # structure dump
cargo run -p exporter --bin gen_dummy                               # PTQ dummy + toy probes
cargo run -p exporter --bin parity_gen                              # parity fixtures
cargo run -p trainer --bin smoke                                    # burn smoke test
```

## Determinism

The whole pipeline is seeded (`SEED = 2026` everywhere, `B::seed` before init
and epoch shuffles, `StdRng` for the split/calibration draws). A re-run
produces a bit-identical `.tflite` (the sha256 is printed and stored in
`model_a.metrics.txt`). No `RAYON_NUM_THREADS` pinning was needed; if a
future backend change breaks this, pin it here and record in NOTES.

## Offline builds

`exporter` builds fully offline (flatbuffers/csv/rand are all in the lock
through the fork). `trainer` needs the network once to fetch `burn`
(pinned `0.21.0`, see the root `Cargo.toml`).
