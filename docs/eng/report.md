# OEE Bench on TinyML: Digital Twin — the report

> The six-week course project, final report. Plan: [`plan.md`](plan.md);
> weekly gates: [`week1-gate.md`](week1-gate.md) …
> [`week6-gate.md`](week6-gate.md). Numbers below are reproducible: every
> table names its one-command source. Host: rustc 1.96.1, linux x86_64;
> QEMU 7.2.22 (`lm3s6965evb`).

## 1. The task, in one paragraph

A digital twin of a production line measures **OEE = Availability ×
Performance × Quality** with three TinyML nodes: node A classifies the
machine current (1D-CNN: idle/run/jam/overload), node P counts parts from
conveyor IR events, node Q judges the tap-test sound (1D-CNN: intact or
cracked). A seeded Rust simulator generates the signals — ground truth is
known by construction, so the twin can be scored. Inference runs on a local
fork of [microflow-rs](https://github.com/matteocarnelos/microflow-rs)
through the project's engine contribution: the **Conv1D operator**
(parser → codegen → int8 kernel). No hardware: the MCU side is emulated
(QEMU LM3S6965), the line is simulated, everything is code.

## 2. Architecture (what was built)

```text
[line-simulator: seeded]  ──current──▶ [node A: 1D-CNN]   ──▶ oee/line1/a/state
  machine FSM, conveyor,    ──IR─────▶ [node P: edges ]   ──▶ oee/line1/p/count
  tap synthesis (WAV-like)  ──taps───▶ [node Q: 1D-CNN]   ──▶ oee/line1/q/verdict
                                                              │
                          [oee-aggregator: watermark windows, A×P×Q]
                                                              │
                          [oee-dashboard: ratatui TUI, live gauges]
```

Six crates in the OEE workspace (`line-simulator`, `features-cli`, `nodes`,
`mqtt-min`, `oee-aggregator`, `oee-dashboard`) plus the ML pipeline
(`ml/trainer` on burn, `ml/exporter`: own PTQ + a TFLite flatbuffers
writer — the models are born in Rust, no Python in the repro loop) and the
engine fork (`fork/microflow`). The seventh crate, `qemu/`, is the week-6
LM3S6965 firmware (not a workspace member — a `thumbv7m` package).

Key design choices that made the numbers trustworthy:

- **One feature source**: `features-cli` computes features for training and
  inference; the models consume raw windows, the features are the analysis
  and parity safety net.
- **The whole ML pipeline is deterministic**: one seed → a bit-identical
  `.tflite` (sha256-pinned in `ml/models/model_a.metrics.txt`).
- **The engine contribution lives in a fork** with golden tests, not in an
  upstream PR (plan section 12).

## 3. The engine contribution (the fork)

**Conv1D end-to-end** — the vertical the engine author describes
(parser → codegen → runtime), spec'd first ([`fork/docs/conv1d-spec.md`](../../fork/docs/conv1d-spec.md)):

1. **Serialization fact (week 1 spike)**: Keras `Conv1D` lands in TFLite as
   `CONV_2D` over a `(1, T, C)` tensor behind a Reshape chain.
2. **The kernel (week 2)**: int8 dot along the time axis, int32 accumulator
   in bias units, per-channel requant, ties-even rounding, saturation on
   cast; Same padding via explicit window geometry. 96 golden fixtures
   (12 shapes × 8 variants) pin it **bit-for-bit** against a naive
   reference (`fork/microflow/tests/golden/`).
3. **The parser/codegen (week 3)**: shape folding (the spike graph folds
   18 ops → 6 layers; `EXPAND_DIMS`/`RESHAPE`/`SHAPE`/… produce no code),
   rank-3 input normalization to `Buffer2D<f32, T, C>`, and the 1-D
   discriminator: `CONV_2D` compiles to `conv_1d` exactly when the filters
   height is 1 AND the effective input height is 1.
4. **Week 6 addition — zero-point hoisting**: the original kernel computed
   `(x − zx)(w − zw)` per element. The benchmark (section 5) showed that
   losing to the generic path, so the accumulator now uses the exact
   integer expansion `Σxw − zx·Σw − zw·Σx + M·zx·zw` (prefix sums for the
   Same-padding edges, static loop bounds in the full-window branch). The
   golden fixtures still pass bit-for-bit — no rounding moved, only the
   instruction order.

Verification chain: golden fixtures (kernel, bit-for-bit) → the exporter's
float interpreter parity (whole model, ≤2 quanta/layer, ±1 end-to-end) →
host/QEMU parity (section 6, bit-for-bit) → the week-5 experiment.

## 4. The twin's correctness (the main numerical result)

The four week-5 scenarios, one command:
`cargo test -p oee-aggregator --test experiment -- --nocapture`
(regenerate: [`week5-gate.md`](week5-gate.md)):

| scenario | seed | true OEE | measured | err    |
| -------- | ---- | -------- | -------- | ------ |
| normal   | 42   | 0.841    | 0.841    | +0.000 |
| downtime | 42   | 0.516    | 0.516    | +0.000 |
| slowdown | 42   | 0.612    | 0.612    | +0.000 |
| rejects  | 42   | 0.478    | 0.478    | +0.000 |

**Conclusion 1**: in-distribution, the twin is exact — each component lands
on its target (A 0.61 under downtime, P 0.75 under the slowdown, Q 0.55 at
p(crack)=0.5) and the measurement tracks it with +0.000 error. The zeros
are the construction working (P exact by the anti-double design; A's
hysteresis shifts transitions equally; Q's model is in-distribution), not a
cooked benchmark.

**Where it actually breaks** (the honest half of the result, week-5 D5
sensitivity sweeps): sub-resolution episodes shorter than
`confirm_after × 80 ms` are lost by A (a 150 ms jam blip is invisible at
depth ≥ 3: +150 ms run-time error); P's anti-double window is exact only in
[70 ms, ~280 ms] (3–4× margin around the 100 ms default); Q degrades
**asymmetrically** under noise shift — the cracked recall dies first, so
measured Q over-reports quality (1.00 while the truth is 0.72 at σ=0.12).

Model quality itself (validation splits, seed 2026;
`ml/models/model_{a,q}.metrics.txt`):

| model | windows | val accuracy (float) | val accuracy (int8) |
| ----- | ------- | -------------------- | ------------------- |
| A     | 2235    | 1.0000               | 1.0000              |
| Q     | 171     | 1.0000               | 1.0000              |

The "too good" risk (plan section 11) materialized here and is bounded by
the sensitivity tables, not hidden: the classes are amplitude-separated by
construction (0.4/2.0/3.2/4.5 A envelopes), so gaussian noise does not move
node A up to 5× the training level.

## 5. The engine's speed (Conv1D vs the reshape trick)

One command: `cargo bench --bench conv1d` in `fork/microflow`
(plus `MICROFLOW_CONV2D_ONLY=1` in its own target dir for the trick rows).
Criterion medians, host:

| benchmark                  | conv_1d   | conv_2d (trick) | speedup |
| -------------------------- | --------- | --------------- | ------- |
| layer 1: 128×1, k3, f8     | 4.04 µs   | 4.14 µs         | 1.02×   |
| layer 2: 63×8, k3, f16     | 4.89 µs   | 15.02 µs        | 3.07×   |
| model A `predict()` (128)  | 14.06 µs  | 23.54 µs        | 1.67×   |
| model Q `predict()` (1024) | 107.95 µs | 186.05 µs       | 1.73×   |

Min/avg/max over 20k runs (`cargo run --release --example latency`):
model A min 15.0 / avg 15.4 µs, model Q min 119.0 / avg 120.5 µs.

**Conclusion 2**: the dedicated kernel wins **1.67–1.73×** end-to-end on
the real node models — dominated by the wider second layer (3.07× there);
the narrow first layer is compute-trivial and the paths tie (1.02×). The
result was not free: the first implementation *lost* by 16–20% until the
zero-point corrections were hoisted (section 3.4) — the benchmark caught
its own contribution's hole.

**The cost of the speed** (flash on LM3S6965, next section): +2.8 KiB
against the trick path — the dual-branch unrolled loops and the prefix-sum
tables. On a 256 KiB part that is 1.1% of flash.

## 6. Portability and footprint (QEMU, LM3S6965)

One command: `scripts/qemu-parity.sh` →
`PARITY OK: 4 windows, bit-for-bit` — the firmware (the same
`model_a.tflite`, compiled for Cortex-M3) and the host reference print
identical probability bits for the fixed windows:

```text
win 0: label=0 argmax=0 idle probs=[0.996 0.000 0.000 0.000] bits=[0x3f7f0000 …]
win 1: label=1 argmax=1 run  probs=[0.000 0.996 0.000 0.000] …
```

**Conclusion 3**: the integer semantics are deterministic across
architectures — host x86_64 and Cortex-M3 produce the same bits, which is
what "portability" means for an int8 engine (and what the digital twin's
reproducibility rests on).

One command: `scripts/footprint.sh` (all three variants run on the board
first; the conv2d rows are checked to match conv1d's predictions):

| variant           | flash B | flash KiB | static RAM B |
| ----------------- | ------- | --------- | ------------ |
| model A, conv_1d  | 46396   | 45.3      | 0            |
| model A, conv_2d  | 43576   | 42.6      | 0            |
| dense floor (toy) | 30328   | 29.6      | 0            |

**Conclusion 4**: node A fits in **45.3 KiB flash / 0 B static RAM** (18%
of the LM3S6965's 256 KiB flash; the runtime stack — a few KiB of
activations — is not in the ELF and the 64 KiB SRAM dwarfs it). The
"dense floor" row is the week-1 serialization toy (random weights, 8-value
input) — it bounds the engine-plus-firmware cost without any conv kernel
(~29.6 KiB), i.e. the two conv layers plus their data cost ~15.7 KiB of
which the dedicated kernel's speed premium is 2.8 KiB.

## 7. Determinism

- One seed → an identical experiment rerun: the week-5 test
  `same_seed_produces_an_identical_result` compares the full windows CSV
  bit-for-bit (`run_id` aside) across two runs with three racing publisher
  threads.
- The train→PTQ→export pipeline re-runs bit-identically (sha256 pinned).
- Host/QEMU parity (section 6) — the same bits on two ISAs.

## 8. Limitations (stated honestly)

- **QEMU is not cycle-accurate** — no MCU timing numbers are claimed from
  it; the speed table is host criterion. QEMU proves portability and
  footprint only.
- **The UART driver relies on QEMU's boot state** (UART0 clocked and wired;
  no GPIO mux / enable sequence) — real silicon needs board init first.
- **The zero-error OEE table is in-distribution truth**: the honest error
  channels are temporal (sub-`confirm_after` episodes), the P window
  margins, and Q's asymmetric degradation under noise shift.
- The loopback broker is QoS 0 without retention/persistence — fine for the
  bench, not a production line.
- The dense comparison row is a size probe, not a trained dense baseline
  (that would need a `ModelDense` in the trainer — future work).

## 9. Future work (plan section 12)

Real hardware (`SensorSource` → `AdcSource`/`I2sSource`, the bench is
bought); the Conv1D PR upstream after the course; the OV2640 camera node;
`MaxPool2D` as the second engine vertical; QAT if int8 accuracy ever drops;
Grafana with history; the Q confirmation-threshold/PR study that the
asymmetry finding asks for.

## 10. Reproduce everything

```bash
cargo test --workspace --release          # 36 suites, the twin + nodes + engine bridge
(cd fork/microflow && cargo test --release)   # the fork: 8 suites incl. the golden fixtures
scripts/qemu-parity.sh                    # host/QEMU bit-for-bit parity
scripts/footprint.sh                      # the flash/RAM table
(cd fork/microflow && cargo bench --bench conv1d)  # the speed table
scripts/bench.sh                          # the live demo: simulator + nodes + dashboard
```
