# oee-qemu — node A on LM3S6965 (QEMU)

The week-6 portability demo: the same int8 model A the host nodes run
(`ml/models/model_a.tflite`) compiled into a `no_std` Cortex-M3 firmware for
the LM3S6965 eval board — the machine the MicroFlow author tests with
(plan section 7). Fixed windows in, class probabilities + argmax out, over
UART0; QEMU exits through semihosting so runs are scriptable.

Not an OEE workspace member: the package is excluded in the root
`Cargo.toml` (a `thumbv7m` crate must not enter host builds) and carries its
own `[patch.crates-io]` for the fork's nalgebra.

## One-time setup

```bash
rustup target add thumbv7m-none-eabi
cargo install flip-link                  # the pinned linker (see .cargo/config.toml)
# QEMU: either a native binary (apt install qemu-system-arm) or the docker
# fallback — scripts/qemu.sh picks the native one when present:
docker build -t oee-qemu qemu/
```

## Commands

```bash
# the demo firmware: 4 fixed windows -> probabilities over UART
cd qemu && cargo run --release --bin oee-qemu

# host/QEMU parity (the D2 gate): builds both sides, diffs bit-for-bit
scripts/qemu-parity.sh

# flash/RAM of the three variants (conv1d / conv2d trick / dense floor)
scripts/footprint.sh
```

The fixed windows come from `ml/models/model_a.val.csv` (the first row of
each class) via `scripts/gen-qemu-windows.py`; the firmware
(`src/windows.rs`) and the host reference (`nodes/examples/qemu_host_ref.rs`)
include the same generated file — the parity compares identical bits.

## The three binaries

| binary     | what it is                                                                                                                                                              |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `oee-qemu` | the demo: model A via the dedicated `conv_1d` kernel (the default `#[model]` path)                                                                                      |
| `conv2d`   | the SAME model forced through the generic `conv_2d` kernel (the reshape trick) — a measurement variant only (`--features footprint-conv2d` + `MICROFLOW_CONV2D_ONLY=1`) |
| `dense`    | the week-1 FC-only serialization toy (random weights) — the engine floor with no conv kernel                                                                            |

## Honest limits

- The UART driver is TX-only, no IRQs, and it relies on QEMU booting the
  board with UART0 already clocked and wired to the serial backend — real
  silicon would need the GPIOA mux and the UART enable sequence first.
- QEMU is not cycle-accurate: the speed numbers come from the host criterion
  benches (`fork/microflow/benches/conv1d.rs`), QEMU proves portability and
  footprint only.
- The `conv2d` variant must build in its own `CARGO_TARGET_DIR` (cargo does
  not fingerprint proc-macro env reads — see `src/bin/conv2d.rs` and
  `scripts/footprint.sh`).
