# firmware/ — node firmware skeletons (the hardware track)

Russian version: [README.ru.md](README.ru.md)

ESP32-S3-DevKitC-1 (N16R8) firmwares for nodes A/P/Q. A separate workspace,
like `fork/microflow`: the target toolchain (Xtensa, `espup`) must not
affect the host CI of the root workspace.

## Status: skeleton without the esp toolchain

All crates build on the host with plain `cargo` (no dependencies). This is
deliberate: the structure and contracts are being pinned down now; the
toolchain gets connected at shakedown time. What is already a contract:

- `board` — bench pins, the single source of truth (a test checks the
  assignments against the S3 reserved-pin list);
- `features-cli` (root workspace) — window/rate contracts (`window_spec`),
  ADC → amps calibration, and the capture CSV schema; the crate is
  `#![no_std]`, firmwares depend on it via a path dependency;
- `nodes::source::SensorSource` — the node data source
  (SimSource/AdcSource/I2sSource).

## Bringing it up (first on-board build)

1. `cargo install espup && espup install` — the patched Xtensa toolchain.
2. `rustup component add rust-src` (for build-std).
3. `. $HOME/export-esp.sh` (espup environment variables).
4. In the node crate, uncomment the dependencies and build:
   `cargo build -p firmware-a --target xtensa-esp32s3-none-elf`.
5. Flash and monitor: `espflash flash` / `espflash monitor`.

Once a real `esp-hal` dependency appears, add a third CI job — build-only
under Xtensa (firmwares have no host tests: a human verifies on hardware).

## Crates

| Crate        | Role                                                            |
| ------------ | --------------------------------------------------------------- |
| `board`      | Bench pins per node + reserved pins (N16R8)                     |
| `firmware-a` | Node A: ACS712 → ADC1 → calibration → window → predict → status |
| `firmware-q` | Node Q: servo tapper → I2S INMP441 → window → predict → verdict |
| `firmware-p` | Node P: TCRT5000 → edge + 50 ms debounce → counting             |

Node Q servo power is a separate 5 V supply (not the board's USB): the
servo inrush current sags the rail and reboots the board; keep a 470 µF
capacitor at the servo pins.
