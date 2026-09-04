# Week 6 Gate — QEMU, Benchmarks, the Report, the Demo

> Implemented 2026-09-04 per [decompose/step-6.md](decompose/step-6.md) on
> branch `feat/qemu-report` (OEE) + `bench/conv1d-vs-conv2d` (fork).
> Checks at formalization: the OEE workspace — 36 suites green, clippy and
> fmt clean; the fork — 8 suites green (the 96 golden fixtures bit-for-bit
> after the week-6 kernel optimization), clippy and fmt clean;
> `scripts/qemu-parity.sh` ends in `PARITY OK: 4 windows, bit-for-bit`.

## Gate checklist

| Gate item                                                                  | Status | Artifact / check                                                                                                                                               |
| -------------------------------------------------------------------------- | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| QEMU: the fork's example and our own model A on LM3S6965, output over UART | yes    | the fork's `examples/qemu` (sine, speech) build for `thumbv7m` and run; `qemu/` firmware: model A, 4 fixed windows, probabilities over UART0, semihosting exit |
| Predictions match the host                                                 | yes    | `scripts/qemu-parity.sh` → `PARITY OK: 4 windows, bit-for-bit` (identical probability bits, host x86_64 vs Cortex-M3)                                          |
| Footprint: flash/RAM from the ELF, Δ conv1d vs trick vs dense              | yes    | `scripts/footprint.sh` — the table below; the conv2d variant's predictions verified identical to conv1d before measuring                                       |
| Criterion: Conv1D vs the Conv2D trick; model min/avg/max latency           | yes    | `fork/microflow/benches/conv1d.rs` (kernels + `predict()`), `examples/latency.rs` (min/avg/max, 20k runs); the tables below                                    |
| The report assembled (results, limitations)                                | yes    | [`report.md`](report.md) (+ [rus pair](../rus/report.md)) — 4 conclusions, each with its table and reproduction command                                        |
| The demo reproduces from a clean clone                                     | yes    | the scenario [`demo.md`](demo.md) (+ rus pair); every beat is one command; the recording itself is the human step (the scenario names the takes)               |

## The numbers (regenerate commands inline)

**Parity (D2)** — `scripts/qemu-parity.sh`:

```text
win 0: label=0 argmax=0 idle probs=[0.996 0.000 0.000 0.000] bits=[0x3f7f0000 0x00000000 0x00000000 0x00000000]
… 4 windows, identical bits on host and QEMU
```

**Footprint (D3)** — `scripts/footprint.sh`:

| variant           | flash B | flash KiB | static RAM B |
| ----------------- | ------- | --------- | ------------ |
| model A, conv_1d  | 46396   | 45.3      | 0            |
| model A, conv_2d  | 43576   | 42.6      | 0            |
| dense floor (toy) | 30328   | 29.6      | 0            |

**Speed (D4)** — `cargo bench --bench conv1d` (+ the `MICROFLOW_CONV2D_ONLY=1`
cross-build for the trick rows):

| benchmark                  | conv_1d   | conv_2d (trick) | speedup |
| -------------------------- | --------- | --------------- | ------- |
| layer 1: 128×1, k3, f8     | 4.04 µs   | 4.14 µs         | 1.02×   |
| layer 2: 63×8, k3, f16     | 4.89 µs   | 15.02 µs        | 3.07×   |
| model A `predict()` (128)  | 14.06 µs  | 23.54 µs        | 1.67×   |
| model Q `predict()` (1024) | 107.95 µs | 186.05 µs       | 1.73×   |

Min/avg/max (20k runs, `examples/latency.rs`): model A 15.0/15.4 µs,
model Q 119.0/120.5 µs.

## What was built (per day of the decomposition)

- **D0** — `rustup target add thumbv7m-none-eabi`; `cargo install
  flip-link` (the fork's pinned linker); QEMU via the `oee-qemu` docker
  image (`qemu/Dockerfile`, qemu-system-arm 7.2.22) with
  `scripts/qemu.sh` preferring a native binary — both branches exist
  (`feat/qemu-report`, `bench/conv1d-vs-conv2d`) from the start.
- **D1** — the fork's `examples/qemu` (sine, speech) built for thumbv7m
  and run under QEMU (semihosting console; they exit cleanly). Version
  pinned here and in the report.
- **D2** — the `qemu/` package (excluded from the workspace, own
  `[patch.crates-io]`): UART0 TX driver, `#[model("../ml/models/
  model_a.tflite")]`, the generated fixed windows
  (`scripts/gen-qemu-windows.py`, the first row of each class from
  `model_a.val.csv`), the byte-stable output format; the host reference
  `nodes/examples/qemu_host_ref.rs` (`nodes::a::classify_with_probs` —
  new); `scripts/qemu-parity.sh` builds both and diffs.
- **D3** — `scripts/footprint.sh`: three ELFs (conv1d / the
  `MICROFLOW_CONV2D_ONLY=1` conv2d cross-build in its own target dir /
  dense), each run on the board first, then `readelf`-based flash/RAM
  classification; the conv2d-vs-conv1d prediction diff gates the table.
- **D4** — `benches/conv1d.rs` (fair kernel A/B: same geometry, same
  weights, identity quant scales; the trick's compile-time constants built
  outside the timed loop) + `predict()` benches for A/Q;
  `examples/latency.rs` (min/avg/max); the trick cross-build documented in
  `fork/NOTES.md` (env + own `CARGO_TARGET_DIR`).
- **D5** — the report ([eng](report.md) / [rus](../rus/report.md)) with the
  four conclusions and their tables.
- **D6** — the demo scenario ([eng](demo.md) / [rus](../rus/demo.md));
  the one-command launch is week-5's `scripts/bench.sh` (unchanged —
  feature freeze).
- **D7** — this gate; the reproducibility section of the report is the
  clean-clone trail (build → test → parity → footprint → bench).

## The mid-week find: the kernel optimization (D4 escalation, applied)

The first bench run showed the dedicated kernel **losing** to the reshape
trick (+16% on layer 2, +20% on whole models): the per-element
`(x − zx)(w − zw)` corrections cost more than the simplified loop saved.
Fixed by an exact integer expansion (zero-point hoisting + kernel-axis
prefix sums + static loop bounds in the full-window branch) — the 96
golden fixtures still pass **bit-for-bit**, and the kernel now wins
1.67–1.73× end-to-end. The flash cost of the speed: +2.8 KiB. This is the
report's "the benchmark caught its own contribution's hole" story.

## Deviations from the decomposition

- **QEMU is dockerized**: the dev box has no `qemu-system-arm` and no
  sudo; `scripts/qemu.sh` wraps the `oee-qemu` image and prefers a native
  binary when present — commands and output are identical, so the
  decomposition's "pin the QEMU versions and commands" holds (7.2.22,
  pinned in the report and here).
- **The fork's examples print over semihosting, not UART** (the
  decomposition assumed UART): D1 ran them as they are; the *own* binary
  (D2) does use the real UART0 — the deviation is documented, not hidden.
- **The dense baseline is a size probe**: `dense.tflite` is the week-1
  serialization toy (random weights, 8-value input), not a trained dense
  model on node-A data — the report says so explicitly; a trained
  `ModelDense` is future work.
- **The demo recording itself** is the human step: the scenario, the
  one-command beats and the backup take are the deliverables here.

## Risks (plan section 11, week-6 view)

- The QEMU-timing risk is handled as planned: no timing claims from QEMU;
  the speed table is host criterion only.
- The "numbers jump between runs" escalation did not materialize:
  criterion medians were stable across reruns; the latency example's max
  column shows scheduler spikes (reported as-is, min/avg are the honest
  columns).

## Retro

- The A/B escape hatch (`MICROFLOW_CONV2D_ONLY`) had to be an env var read
  inside the proc macro — and cargo does not fingerprint those reads. Two
  target dirs and a written-down trap note (the week-6 twin of the week-3
  toolchain trap) keep the measurements honest; a cargo feature would have
  been silently union-merged across the bins.
- Building the firmware from the package directory (not `--manifest-path`
  from the root) is load-bearing: `.cargo/config.toml` (flip-link,
  `-Tlink.x`) is discovered from the invocation directory. Cost one
  debugging round; documented in `scripts/footprint.sh`.
- The generated windows file tripped clippy (`approx_constant`: a sample
  sat near 1/π) and rustfmt (long data lines). Generated data wants
  `#![allow(clippy::approx_constant)]` + `#[rustfmt::skip]` — now emitted
  by the generator itself, so regeneration cannot regress it.
