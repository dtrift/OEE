# Hardware track — decomposition (ESP32-S3 firmware, bench shakedown)

> Branch: `feat/firmware-bringup` (the `firmware/` workspace + CI only; on an
> inference bug — a fork hotfix branch, as in week 4).

> Breakdown of the "hardware track" from the plan [`plan.md`](../plan.md),
> sections 2 and 12. The track is outside the weeks 1–6 calendar: it runs in
> parallel, the course's critical path does not wait for it (the section 2
> rule). Format — sessions of ~2–3 h (one theme per session, the section 8
> rule), estimate ~20–24 h. The bench: 2× ESP32-S3-DevKitC-1 (N16R8) — nodes
> A and Q; 1× ESP32-S3-WROOM-1 N16R8 CAM with an on-board OV2640 — node P
> and the stretch camera; the sensors ACS712-20A, INMP441, TCRT5000, a servo
> tapper, a separate 5 V supply. Bench details and the bring-up procedure —
> [`firmware/README.md`](../../../firmware/README.md).

> Input: the contracts are already in the repo — `features-cli`
> (`window_spec`, the ADC→amps calibration, the `capture` schema),
> `nodes::source::SensorSource` (`AdcSource`/`I2sSource`/`GpioEdgeSource`),
> `board` (bench pins + a test), the `firmware-{a,q,p}` skeletons (they build
> on the host without the toolchain), the rust-born models
> `model_a.tflite`/`model_q.tflite` (`#[model]`, no_std) and the week-4 host
> pipelines (`nodes` — the behavior reference). The week-6 QEMU demo is a
> separate line (a different target), not mixed with this track.

## Track gate (minimum done)

- [ ] `firmware-a` on a bench run: the statuses match the host node A on the
      same data (mismatches — only at window boundaries, as in week 4).
- [ ] `firmware-q`: the verdicts on reference parts (good/cracked) match the
      host model on the same windows.
- [ ] `firmware-p`: the part count equals the run's fact; bouncing is
      debounced away.
- [ ] The capture CSV from the board is readable by the host tooling (a
      capture→run converter + node A); captures are reproducible.
- [ ] CI: a third build-only job under `xtensa-esp32s3-none-elf` is green.
- [ ] `firmware/NOTES.md` with the shakedown facts; the phase-2 decision
      (UART bridge vs Wi-Fi) recorded in [firmware-gate.md](../firmware-gate.md).

## Session summary

| Step | Session theme | Gist                                             | Artifact                       |
| ---- | ------------- | ------------------------------------------------ | ------------------------------ |
| S0   | toolchain     | espup, blinky, a build-only CI job               | blinky on board, green job     |
| S1   | node A        | ADC1 + ACS712, calibration, capture over UART    | capture CSV, zero in tolerance |
| S2   | node A        | window → `#[model]` → hysteresis → status        | A statuses over UART           |
| S3   | cross-check   | the same run: board vs the host node A           | a mismatch table in NOTES      |
| S4   | node Q        | I2S INMP441, a 1024 window, a spectrum dump      | the tone in the expected bin   |
| S5   | node Q        | the servo tapper, `#[model] model_q`, verdicts   | verdicts on reference parts    |
| S6   | node P        | TCRT5000, a 50 ms debounce, counting             | count = the run's fact         |
| S7   | gate          | shakedown NOTES, firmware-gate, the phase-2 call | `docs/firmware-gate.md`        |

## S0 (~2 h) — toolchain: espup, blinky, CI

1. `espup install` + `rustup component add rust-src` + `. $HOME/export-esp.sh`
   (bring-up steps 1–3 from firmware/README).
2. In `firmware-a`, uncomment the dependencies (`board`, `features-cli`,
   `esp-hal 1.1.2`) — step 4; a blinky on a `board` pin (the board is alive,
   the pins match the schematic).
3. CI: a third build-only job `cargo build -p firmware-a --target
   xtensa-esp32s3-none-elf` (no tests — the README rule: a human verifies on
   hardware; unit tests stay with pure logic, the `board` precedent).

Check: the blinky blinks; the CI job is green in a PR.

## S1 (~3 h) — node A: ADC and calibration

1. `AdcSource`: a `SensorSource` implementation over ADC1 (ACS712), sampling
   at ≥ `window_spec(A).sample_rate_hz` (1.6 kHz, a timer); the pins — from
   `board`.
2. Startup zero recalibration: averaging at rest →
   `CurrentCalibration::with_zero_counts` (ACS712 zero drift is an expected
   fact; the contract is ready).
3. Capture: capture-CSV lines (`t_ms,node,run_id,value,state,note`) over
   UART; a host terminal writes them to a file.

Check: at rest ≈ 0 A (within the calibration tolerance); a multimeter vs the
readings is linear at 2–3 current levels.

## S2 (~3 h) — node A: window → predict → status

1. Wire the fork from the firmware workspace: a path dependency
   `../fork/microflow` + the duplicated `[patch.crates-io] nalgebra` in
   `firmware/Cargo.toml` (the root manifest convention, week 3/D6).
2. The `WindowSpec(A)` window = 128 @ 1.6 kHz →
   `#[model("ml/models/model_a.tflite")]` → the ×2 hysteresis. The window/
   hysteresis logic — a compact copy from `nodes::status` (~50 lines,
   `no_std` + alloc) with a host unit test of equivalence; extracting a
   shared crate — only via the escalation path.
3. The status over UART as an `a,<run_id>,<t_ms>,<state>` line (the week-4
   offline-CSV family) — on confirmed changes, not per window.

Check: across the bench's regular regimes the statuses follow the regime;
mismatches — only at boundaries (the analog of the week-4 D1 check).

## S3 (~2 h) — cross-check: the twin vs the hardware

1. A capture→run CSV converter on the host (a small utility: renaming the
   `value→current_a` column, filtering `node=a`) — the hardware captures
   become the input of the host `node --kind a`.
2. One and the same physical run: capture from the board → the capture CSV →
   the host node; a status mismatch table → `firmware/NOTES.md`.

Check: the statuses match away from window boundaries; otherwise — a fix
(calibration/threshold), the cause and the resolution in NOTES.

## S4 (~3 h) — node Q: the I2S microphone

1. `I2sSource`: INMP441, 16 kHz (`window_spec(Q)`), a DMA ring; the frames →
   f32 with a pinned normalization.
2. A dump of a 1024 window over UART on command; the spectrum on the host
   (serial instruments).

Check: a tone of a known frequency (a generator/an app) lands in the expected
bin within ±5%; the window is exactly 64 ms.

## S5 (~3 h) — node Q: the tapper and the verdicts

1. The servo: a PWM strike pulse; the "strike → record 64 ms" synchronization
   (the window starts at the strike, as in the simulator's `taps.rs`).
2. `#[model("ml/models/model_q.tflite")]` → the verdict over UART
   (`q,<run_id>,<t_ms>,verdict`).
3. The servo power — strictly a separate 5 V supply + 470 µF at the pins (the
   firmware/README condition: a sag reboots the board).

Check: reference parts (≥5 good, ≥5 cracked) — the verdicts match the part's
class; the mismatches go to NOTES.

## S6 (~2 h) — node P: part counting

1. `GpioEdgeSource`: TCRT5000, the part-passage edge + a 50 ms debounce (the
   firmware/README parameter), a counter. Node P lives on the CAM board
   (ESP32-S3 WROOM N16R8 CAM): pick a pin free of the camera and the
   reserved list (check your board's camera wiring against its schematic —
   CAM boards differ in the layout).
2. The count over UART on change (`p,<run_id>,<t_ms>,count`).

Check: a run of N parts → the count = N; bouncing/repeats give no false
triggers.

## S7 (~2–3 h) — the gate and NOTES

1. `firmware/NOTES.md`: the shakedown facts (the ADC cadence and noise, the
   zero offset, the I2S byte/channel order, the servo current, P's bounce).
2. The gate doc [firmware-gate.md](../firmware-gate.md) (rus/eng, after the
   weekly-gate pattern): a checklist, deviations, retro.
3. The phase-2 decision: a host UART bridge → mosquitto (the fast path, the
   bench joins the week-5 full loop with the aggregator) vs `esp-wifi` + a
   `mqtt-min` port (future work).

## Escalation points

- The espup toolchain does not build / the board does not flash → the whole
  track shifts: the contracts are already pinned (the week-1 insurance), the
  course is not blocked; return after week 6.
- The ADC noise is above expectations → the startup recalibration +
  averaging; next — an RC hardware filter (a fix in the schematic/`board`,
  not in the contracts).
- The I2S stream arrives with the wrong order/endianness → dump the raw
  window, compare against the reference, fix locally in `I2sSource`; the
  `SensorSource` contract does not change.
- The on-board model diverges from the host by more than ±1 quantum → a fork
  hotfix branch (the week-3 precedent) + the question: why did the parity
  test not catch it; close the hole with a test.
- The servo sags the rail → that is a hardware problem (a separate supply,
  470 µF), not fixed in software.
- MQTT from the bench is urgently needed → a host UART bridge (publishing
  `oee/line1/*` from the status lines); porting `mqtt-min` to no_std —
  future work.

## Anti-scope (what this track does NOT do)

- The aggregator and the dashboard are not ported to the board — they stay
  host-side (weeks 5–6).
- OTA, on-board history storage, network configuration — no.
- Training/PTQ on the board — no; the models are born in `ml/` and baked by
  `#[model]`.
- Changing the `features-cli` contracts for the hardware — only by a separate
  decision, keeping the host consumers working.
- The week-6 QEMU line (`thumbv7m`) is not mixed in here: a different target,
  a different artifact.

## Phase 2 (after the gate, by a separate decision)

- The UART bridge: a host utility reads the A/P/Q status lines and publishes
  them to `mosquitto` on `oee/line1/*` — the bench enters the digital twin's
  full loop (the week-5 aggregator/dashboard) without a single line of
  network code on the boards.
- `esp-wifi` + porting `mqtt-min` (the aggregator's SUBSCRIBE too) — a
  candidate for the report's future work.
