# firmware/ — node firmwares (the hardware track)

Russian version: [README.ru.md](README.ru.md). The shakedown plan (sessions
S0–S7, the gate): [docs/eng/decompose/firmware.md](../docs/eng/decompose/firmware.md).

ESP32-S3 firmwares for nodes A/P/Q. The bench: 2× ESP32-S3-DevKitC-1 (N16R8)
— nodes A and Q; 1× ESP32-S3-WROOM-1 N16R8 **CAM** with an on-board OV2640 —
node P plus the stretch camera (its camera wiring takes some pins). A
separate workspace, like `fork/microflow`: the target toolchain (Xtensa,
`espup`) must not affect the host CI of the root workspace.

## Status: implemented, awaiting bench bring-up

The track is implemented in code; the physical bring-up (S0 blinky → S6
counting) is the remaining human-on-hardware part:

- **`firmware-{a,q,p}`** build for `xtensa-esp32s3-none-elf` (esp-hal 1.2,
  the `unstable` driver modules) **and** on the host (an empty stub binary;
  the node logic is the host-tested lib of each crate):
  - `a`: ADC1@GPIO4 1.6 kHz → startup zero calibration (`with_zero_counts`)
    → window 128 → `model_a` → hysteresis ×2 → `a,run_id,t_ms,state` lines;
  - `q`: servo PWM (LEDC 50 Hz, GPIO11) → settle → window 1024 → `model_q`
    → `q,run_id,t_ms,verdict` lines. **S4 TODO**: the window is a synthetic
    tap until the I2S/INMP441 driver lands (`i2s_slots_to_f32` in the lib
    is pinned by host tests; a tone-check dump is the bring-up step);
  - `p`: TCRT5000@GPIO5, 1 kHz poll → 50 ms debounce → counter →
    `p,run_id,t_ms,count` lines.
- **`firmware-tools`** (host): `capture-to-run` (the board capture CSV →
  the host node input, the S3 cross-check) and `uart-bridge` (stdin lines →
  MQTT `oee/line1/*` + end markers — phase 2, the week-5 loop against the
  physical bench without network code on the boards).
- **CI**: two jobs — `firmware` (Xtensa build-only) and `firmware-host`
  (fmt + clippy + the lib tests).

## Building

Host (no toolchain needed — logic tests):

```bash
cd firmware && cargo test --workspace
```

Target (once per shell):

```bash
cargo install espup && espup install   # the patched Xtensa toolchain
. $HOME/export-esp.sh                  # the linker (xtensa-esp-elf-gcc) PATH
```

The esp toolchain lives in `~/.rustup/toolchains/esp` (espup's default),
which the asdf-managed shell rustup does not see — hence the full path:

```bash
cd firmware
~/.rustup/toolchains/esp/bin/cargo build \
    -p firmware-a -p firmware-q -p firmware-p \
    --target xtensa-esp32s3-none-elf
```

Flash and monitor — see the next section.

## Flashing the boards

One ELF per board (espflash converts it into a bootable image itself — no
separate .bin to prepare). Use the **release** builds; match the binary to
the board's wiring (`board` is the single source of truth):

| Board                                 | Node | File (`firmware/target/xtensa-esp32s3-none-elf/release/`) |
| ------------------------------------- | ---- | --------------------------------------------------------- |
| DevKitC-1 #1 (ACS712 on GPIO4)        | A    | `firmware-a`                                              |
| DevKitC-1 #2 (servo GPIO11 + INMP441) | Q    | `firmware-q`                                              |
| CAM board (TCRT5000 on GPIO5)         | P    | `firmware-p`                                              |

The boards differ by their USB serial port — check `/dev/ttyUSB*`:

```bash
espflash flash --port /dev/ttyUSB0 target/xtensa-esp32s3-none-elf/release/firmware-a
espflash flash --port /dev/ttyUSB1 target/xtensa-esp32s3-none-elf/release/firmware-q
espflash flash --port /dev/ttyUSB2 target/xtensa-esp32s3-none-elf/release/firmware-p
```

`--monitor` right after flashing shows the console immediately. If a
board is not seen, hold **BOOT** while plugging it in (the USB download
mode).

What a live image prints over UART (115200) — the first bring-up check:

- A: `a: boot, run_id=bench-a` → `a: zero=NNN` (the startup zero
  calibration) → `a,bench-a,<t_ms>,<state>` lines on confirmed changes;
- Q: `q: boot, run_id=bench-q` → `q,bench-q,<t_ms>,<verdict>` every
  ~400 ms (a synthetic window until the S4 I2S step);
- P: `p: boot, run_id=bench-p` → `p,bench-p,<t_ms>,<count>` per part.

Do NOT flash:

- `qemu/target/.../oee-qemu` — the week-6 LM3S6965/Cortex-M3 artifact,
  a different toolchain and target; it will not boot on an ESP32-S3;
- the debug builds work but are 10× larger for no bring-up benefit
  (panics print their UART line in release too).

## The contracts in play

- `board` — bench pins, the single source of truth (a test checks the
  assignments against the S3 reserved-pin list);
- `features-cli` (root workspace) — window/rate contracts (`window_spec`),
  the ADC → amps calibration (incl. the runtime zero correction), the
  capture CSV schema; `#![no_std]`, used here via a path dependency;
- `nodes::source::SensorSource` — the node data source contract (the host
  `SimSource` twin); the firmware libs mirror the host `nodes::status`
  semantics (window/hysteresis), pinned by tests incl. a model-parity
  fixture against the real validation split.

## Crates

| Crate            | Role                                                                |
| ---------------- | ------------------------------------------------------------------- |
| `board`          | Bench pins per node + reserved pins (N16R8; the CAM board — node P) |
| `firmware-a`     | Node A: ACS712 → ADC1 → calibration → window → predict → status     |
| `firmware-q`     | Node Q: servo tapper → I2S INMP441 → window → predict → verdict     |
| `firmware-p`     | Node P (the CAM board): TCRT5000 → edge + 50 ms debounce → counting |
| `firmware-tools` | Host bench tools: `capture-to-run`, `uart-bridge`                   |

Node Q servo power is a separate 5 V supply (not the board's USB): the
servo inrush current sags the rail and reboots the board; keep a 470 µF
capacitor at the servo pins.

## The bench loop (phase 2, after bring-up)

```bash
# terminal 1: the broker + the week-5 aggregator/dashboard
./target/debug/broker 1883 &
./target/debug/aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 --out windows.csv
./target/debug/oee-dashboard --mqtt 127.0.0.1:1883
# terminal 2: the boards' lines into MQTT
cat /dev/ttyUSB0 | cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```
