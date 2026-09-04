#!/bin/sh
# Host/QEMU parity check (week 6, D2 gate).
#
# One command: builds the LM3S6965 firmware and the host reference, runs
# both over the SAME fixed windows (qemu/src/windows.rs) and diffs the
# byte-stable prediction lines. Identical output = the integer semantics of
# the kernel are deterministic across host and Cortex-M3 builds (spec 3).
#
# Artifacts: tmp/qemu/uart.log, tmp/qemu/host.log.

set -eu
cd "$(dirname -- "$0")/.."

OUT=tmp/qemu
mkdir -p "$OUT"

echo "== building the firmware (thumbv7m, release)"
(cd qemu && cargo build --release --bin oee-qemu)

echo "== building the host reference"
cargo build -q --release -p nodes --example qemu_host_ref

echo "== firmware under QEMU (LM3S6965, UART0 -> stdio)"
scripts/qemu.sh -cpu cortex-m3 -machine lm3s6965evb -nographic \
    -semihosting-config enable=on,target=native \
    -kernel qemu/target/thumbv7m-none-eabi/release/oee-qemu \
    >"$OUT/uart.log" 2>&1
cat "$OUT/uart.log"

echo "== host reference"
./target/release/examples/qemu_host_ref >"$OUT/host.log"
cat "$OUT/host.log"

echo "== diff (empty = bit-for-bit parity)"
# Drop the QEMU boot line ("Timer with period zero, disabling") and the
# banner; compare only the win/done lines, which both sides emit verbatim.
grep -E '^(win|done)' "$OUT/uart.log" >"$OUT/uart.predictions"
grep -E '^(win|done)' "$OUT/host.log" >"$OUT/host.predictions"
if diff "$OUT/host.predictions" "$OUT/uart.predictions"; then
    echo "PARITY OK: $(grep -c '^win' "$OUT/uart.predictions") windows, bit-for-bit"
else
    echo "PARITY FAILED" >&2
    exit 1
fi
