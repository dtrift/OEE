#!/bin/sh
# Footprint of the three firmware variants (week 6, D3, plan section 10).
#
#   conv1d — model A through the dedicated conv_1d kernel (the fork's
#            contribution, the default `#[model]` path)
#   conv2d — the SAME model through the generic conv_2d kernel: the
#            "reshape trick" a pre-Conv1D engine would run. Forced by
#            MICROFLOW_CONV2D_ONLY=1 at macro-expansion time.
#   dense  — the week-1 FC-only serialization toy (random weights): the
#            engine floor with no convolution kernel at all.
#
# Prints a markdown table (also saved to tmp/footprint/table.md) and checks
# that conv2d and conv1d produce IDENTICAL predictions over UART (the A/B
# must not change semantics — only the code path).
#
# Build trap (see qemu/src/bin/conv2d.rs): cargo does not fingerprint
# proc-macro env reads, so the conv2d variant builds in its own target dir.

set -eu
cd "$(dirname -- "$0")/.."

OUT=tmp/footprint
mkdir -p "$OUT"

FLASH_BASE=$((0x00000000))
RAM_BASE=$((0x20000000))

# readelf-based classifier: allocatable sections, region by address.
# flash = load image (flash sections + .data), ram = .data + .bss + statics.
footprint() {
    readelf -SW "$1" | awk -v flash_base="$FLASH_BASE" -v ram_base="$RAM_BASE" '
        $1 == "[" && $9 ~ /A/ {
            addr = strtonum("0x" $5)
            sz = strtonum("0x" $7)
            if (addr >= ram_base) {
                ram += sz
                if ($3 == ".data") data = sz
            } else if (addr >= flash_base) {
                flash += sz
            }
        }
        END { printf "%d %d\n", flash + data, ram }
    '
}

echo "== building the three variants"
# All builds run from qemu/: .cargo/config.toml (flip-link, -Tlink.x) is
# discovered from the cargo invocation directory, not the manifest path.
(cd qemu && cargo build -q --release --bin oee-qemu --bin dense)
(cd qemu && MICROFLOW_CONV2D_ONLY=1 CARGO_TARGET_DIR="$PWD/../$OUT/target-conv2d" \
    cargo build -q --release --bin conv2d --features footprint-conv2d)

CONV1D_ELF="qemu/target/thumbv7m-none-eabi/release/oee-qemu"
CONV2D_ELF="$OUT/target-conv2d/thumbv7m-none-eabi/release/conv2d"
DENSE_ELF="qemu/target/thumbv7m-none-eabi/release/dense"

echo "== sanity: every variant runs on LM3S6965 and prints over UART"
for elf in "$CONV1D_ELF" "$CONV2D_ELF" "$DENSE_ELF"; do
    name="$(basename "$elf")"
    scripts/qemu.sh -cpu cortex-m3 -machine lm3s6965evb -nographic \
        -semihosting-config enable=on,target=native -kernel "$elf" \
        >"$OUT/$name.log" 2>&1
    grep -q '^done\|^dense:' "$OUT/$name.log" || {
        echo "!! $name produced no output" >&2
        exit 1
    }
done

echo "== semantics check: conv2d must match conv1d bit-for-bit"
grep '^win' "$OUT/oee-qemu.log" >"$OUT/conv1d.predictions"
grep '^win' "$OUT/conv2d.log" >"$OUT/conv2d.predictions"
if diff "$OUT/conv1d.predictions" "$OUT/conv2d.predictions"; then
    echo "identical predictions through both code paths"
else
    echo "!! the conv2d variant changed the predictions" >&2
    exit 1
fi

echo "== flash/RAM from the ELFs"
{
    echo "| variant | flash B | flash KiB | static RAM B | ELF B |"
    echo "| ------- | ------- | --------- | ------------ | ----- |"
    for pair in "conv1d:$CONV1D_ELF" "conv2d:$CONV2D_ELF" "dense:$DENSE_ELF"; do
        name="${pair%%:*}"
        elf="${pair#*:}"
        read -r flash ram <<EOF
$(footprint "$elf")
EOF
        elf_size="$(stat -c %s "$elf")"
        echo "| $name | $flash | $(awk "BEGIN{printf \"%.1f\", $flash/1024}") | $ram | $elf_size |"
    done
} | tee "$OUT/table.md"
