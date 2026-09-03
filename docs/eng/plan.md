# OEE Bench on TinyML: Digital Twin (Code-Only, No Hardware)

> Mode: 1 person. Duration: 6 weeks (cut-line at week 4).
> Code only: the physical bench is replaced by a Rust simulator, MCU metrics — via QEMU.
> No upstream PRs: the contribution lives in a local fork of [microflow-rs](https://github.com/matteocarnelos/microflow-rs).
> Previous (hardware) version of the plan: [`OEE.md`](../../kontext/OEE.md).

## 0. Description for the mentor (one sentence)

**Full version:**

> Rust course project — a digital twin of a production line with OEE measurement on TinyML:
> a Rust simulator generates the machine current signal, conveyor events, and tap-test part sound;
> node processes run inference of quantized neural networks through a MicroFlow fork (contribution —
> implementing the `Conv1D` operator: parser → codegen → int8 runtime), and OEE = Availability ×
> Performance × Quality converges into a live MQTT dashboard and is cross-checked against the
> simulator's ground truth.

**Short version:**

> A Rust production-line simulator measures OEE with three TinyML nodes (current, counting, sound)
> via a MicroFlow fork with the `Conv1D` operator implemented by me; inference — code and QEMU only,
> no hardware.

## 1. Concept and Math

### 1.1 What OEE is

OEE (Overall Equipment Effectiveness) is a standard manufacturing metric: how efficiently a machine
or line is used relative to its ideal maximum. It is expressed as a single number from 0 to 100%:

**OEE = Availability × Performance × Quality**

Each factor is a separate question about production:

| Component    | Question                    | Formula                          | Example                              |
| ------------ | --------------------------- | -------------------------------- | ------------------------------------ |
| Availability | Did the machine run at all? | Run Time / Planned Time          | 1 h down out of an 8 h shift → 87.5% |
| Performance  | Did it run at full speed?   | (Ideal Cycle × Count) / Run Time | ideal 100 parts/h, made 85 → 85%     |
| Quality      | Were the parts good?        | Good Parts / Total Parts         | 3 failed out of 100 parts → 97%      |

Example: 0.875 × 0.85 × 0.97 ≈ **72%** OEE.

Why one number instead of three: a machine can be "running perfectly" yet OEE is still low —
it stood idle half the shift (Availability loss), ran below nominal speed (Performance),
or turned out some failed parts (Quality). OEE collapses these three kinds of hidden losses into one
dashboard metric. Reference points: **85%** — world class (first achieved by Toyota), **60%** —
typical unoptimized production, **40%** — a low-automation shop floor.

### 1.2 OEE in this project

Each component is measured by TinyML classification on simulated data:

| Component    | Formula                  | Node             | Data source in the simulator                                            |
| ------------ | ------------------------ | ---------------- | ----------------------------------------------------------------------- |
| Availability | Run Time / Planned Time  | A — current (ML) | 1D-CNN over a synthetic current signal: running / idle / jam            |
| Performance  | Ideal Cycle × Count / RT | P — counting     | "part passed" events from the conveyor (IR-barrier edge detector)       |
| Quality      | Good / Total             | Q — sound (ML)   | synthesized tap-test sound: an intact part rings, a cracked one is dull |

The result is a single OEE number on a live dashboard. Ground truth is known to the simulator by
construction: comparing measured vs true OEE is the main numerical result.

## 2. Key decisions (what replaced the hardware)

- **Rust line simulator** — deterministic (seeded RNG), reproducible experiments.
- **Nodes as host processes**: the same pipeline (sensor sim → features → `predict()` → MQTT) as
  on the firmware, but without an MCU; the architecture via the `SensorSource` trait
  (`SimSource` now, `AdcSource` in future work) keeps the door open for real hardware.
- **A parallel hardware track** — off the critical path: contracts in `features-cli`
  (`window_spec`, calibration, `capture`), the `SensorSource` trait, and the `firmware/` skeleton
  (ESP32-S3) prepare a run on the real bench; the plan's schedule and gates do not change
  (details — README, "Hardware track" section).
- **QEMU (LM3S6965)** — the standard machine in stock QEMU for Embedded Rust: exact flash/RAM from
  the ELF, approximate timings (state this honestly in the report).
- **No upstream PRs**: the engine contribution lives in a local fork; benchmarks and tests go in
  the report.
- Node Q (acoustics) stays: sound synthesis is pure code; the dataset does not need to be recorded
  with a drill.

## 3. Digital twin architecture

```text
[line-simulator: Rust, seeded]
  machine (FSM: idle/run/jam) → current signal ──→ [node A] features → 1D-CNN → status
  conveyor (speed, intervals) → IR events     ──→ [node P] edge detector → count
  tap test (ring synthesis) → WAV-like buffer → [node Q] features → 1D-CNN → pass/fail
                                                    │
                              MQTT: oee/line1/{state,count,verdict}
                                                    │
                              [oee-aggregator: A×P×Q → oee-dashboard (ratatui TUI)]
```

Repository layout (workspace):

- `line-simulator/` — scenario and signal generator; writes raw data to CSV/binaries
  (this doubles as the training dataset) and streams to the nodes.
- `nodes/` — nodes A/P/Q: data intake, feature extraction, `#[model]` inference, MQTT publishing.
- `oee-aggregator/` — MQTT subscription, OEE formula, publication for the dashboard.
- `oee-dashboard/` — TUI dashboard (ratatui): live OEE/A/P/Q, counter, Q verdicts.
- `features-cli/` — **the same Rust feature code** for training (exported to Python) and for
  inference: feature parity by construction, not by racing numpy.
- `fork/microflow` — the engine fork with `Conv1D` (git subtree or submodule).

## 4. Line simulator: what exactly we model

- **Machine** — a finite state machine `idle → run → jam/overload` driven by a scenario; a scenario
  is a declarative event list (t=120s: jam for 40s; t=600s: slowdown, etc.), which also serves as
  ground truth.
- **Current signal** — a synthetic waveform: base 50 Hz sine + amplitude envelope by mode +
  harmonics + noise (seeded RNG). Classes are distinguishable, but not perfectly — otherwise ML is
  pointless.
- **Conveyor** — a part passes the IR barrier with jitter; sometimes two in a row (edge case:
  anti-double-count), sometimes a miss.
- **Tap test** — sound synthesis: a decaying sine; an intact part — high frequency, long decay;
  a cracked one — lower frequency, fast decay + noise. Parameters are varied by seeded RNG.
- **Reproducibility**: one seed → the same dataset and experiment; the report includes a table
  "scenario → true OEE → measured OEE → error".

## 5. Engine contribution: `Conv1D` (local fork)

Week 1 nuance (spike): Keras `Conv1D`, when exported to TFLite, is usually serialized as `CONV_2D`
over a `(1, T, C)` tensor with a `Reshape` chain. Hence the work:

1. Teach `microflow-macros` to understand this chain and 3D tensors (flatbuffers parser).
2. An efficient int8 kernel for the 1D case: dot product + int32 accumulator + requant.
3. Unit tests against a reference model (golden vectors from the Rust `golden-gen`, same toolchain;
   numpy is not used — a week 1 decision, spec §5.2).
4. Benchmark (criterion): "honest Conv1D vs the reshape trick through Conv2D" — time, flash/RAM.

The vertical is the same as the engine author's: parser → codegen → runtime → tests. No upstream
commitments: a clean experiment in the fork + a benchmarks section in the report.

## 6. ML pipeline (Python side)

1. Datasets are generated by `line-simulator` (the same CSVs that will go to the nodes for
   inference).
2. Models only from supported operators: `Conv1D(8)→AvgPool→Conv1D(16)→AvgPool→FC→Softmax`.
3. Full-integer int8 quantization (post-training), export `.tflite`.
4. Feature parity — the main pain point in TinyML — is solved here architecturally: features for
   training and for inference are computed by one Rust crate (`features-cli`); numpy receives
   ready-made features. Golden tests remain as a safety net.

## 7. QEMU: "MCU without an MCU"

- The target machine **LM3S6965** (`qemu-system-arm -M lm3s6965evb`) is officially tested by the
  MicroFlow author and present in stock QEMU; toolchain `thumbv7m-none-eabi`.
- Run examples and our own models with UART output; metrics: flash/RAM from the ELF (exact),
  latency (approximate — state in limitations).
- Host criterion benchmarks are the primary source of speed numbers; QEMU is about portability
  and footprint.

## 8. Working mode

- One topic per session: engine / ML / simulator / integration — do not mix.
- Order within a topic: spec → code → tests, not "write Conv1D right away".
- All code passes `cargo test` + `cargo clippy` — before moving to the next topic.

## 9. Week-by-week plan (6 weeks, gates)

| Week | What we do                                                                                                                                   | Gate (minimal ready state)                            |
| ---- | -------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| 1    | Spike: building the MicroFlow fork on the host; a test Keras model → flatbuffers — how `Conv1D` serializes; workspace and simulator skeleton | `predict()` on the host; the `Conv1D` spec            |
| 2    | Int8 `Conv1D` kernel + unit tests (host); simulator: machine FSM + current signal                                                            | kernel passes golden tests against the Rust reference |
| 3    | `Conv1D` in `microflow-macros` (parser + codegen); Python: training on synthetic data, int8; parity tests                                    | host inference of a real `.tflite` via `#[model]`     |
| 4    | Node A end-to-end (sim → features → predict → MQTT); node Q (sound synthesis + model + integration)                                          | A and Q statuses in MQTT                              |
| 5    | Node P; OEE aggregator + ratatui TUI dashboard (fallback — Node-RED); downtime/failures → measured vs true OEE                               | "measured vs truth" table on the dashboard            |
| 6    | QEMU run (LM3S6965): flash/RAM, UART demo; criterion benchmarks; report; demo recording                                                      | everything builds, the demo is reproducible           |

**Cut-line (deadline at the end of week 4)**: drop node Q → OEE from A+P, Quality = 1.0
(baseline); the OEE number survives, acoustics — future work. There is no reserve week — that is
why the gates are phrased as "minimal ready state", and weeks 1–3 (engine) are the most protected:
they are entirely on the host.

## 10. Metrics for the report (the "results" section)

- **OEE correctness**: a series of scenarios (normal / downtime / slowdown / failures); a table
  "true OEE → measured OEE → error"; the main numerical result.
- **Engine**: latency (min/avg/max) of models on the host and in QEMU, Δ flash/RAM,
  "Conv1D vs the reshape trick" (criterion).
- **Nodes**: confusion matrix for A and Q, PR curve for the failure threshold, robustness to seeds.
- **Determinism**: one seed → an identical rerun of the experiment (verified by running twice).

## 11. Risks and their handling

- **Building MicroFlow on the host** (dependencies, Rust versions) — week 1 risk #1; if the `sine`
  example does not build within an evening — an early signal, reshuffle the plan immediately.
- **TFLite parser**: the `Conv1D → Reshape → CONV_2D` chain may prove harder than expected — that
  is exactly why the spike is scheduled for week 1, before writing the kernel.
- **"Too good" models**: synthetic data without real noise gives ~100% accuracy — fixed by varying
  noise/parameters in the simulator and a sensitivity analysis in the report.
- **QEMU timings are not cycle-accurate** — state honestly in limitations; the main speed numbers
  come from host criterion benchmarks.
- **Feature parity** — solved architecturally (a shared Rust features crate), golden tests as a
  safety net.
- **You are the single point of failure** — "minimal ready state" gates, cut-line at week 4.

## 12. Future work (deliberately out of scope)

- Real hardware: the `SensorSource` trait is already prepared for `AdcSource`/`I2sSource`
  (see README, "Hardware track"; the bench is bought: 2× DevKitC-1 + an
  ESP32-S3 WROOM N16R8 CAM board with OV2640 — the `decompose/firmware.md`
  breakdown).
- A `Conv1D` PR upstream after the course (code and tests are already in the fork).
- The OV2640 camera (the module is already on the bench — the ESP32-S3 WROOM
  N16R8 CAM board, node P) + `MaxPool2D` (a second engine vertical).
- QAT if int8 accuracy drops; Grafana with history instead of the TUI dashboard.
