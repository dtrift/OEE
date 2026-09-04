#!/bin/sh
# qemu-system-arm for the LM3S6965 demo (week 6, D0).
#
# Native binary if present (apt install qemu-system-arm); otherwise the
# oee-qemu docker image (qemu/Dockerfile) — the commands and output are
# identical either way, so the demo reproduces from a clean clone with or
# without a system-wide QEMU.
#
# Version pinning (the report quotes it): native `qemu-system-arm --version`;
# the image pins Debian bookworm's qemu-system-arm 7.2.x — this script prints
# the version line into the log on every run.

set -eu

if command -v qemu-system-arm >/dev/null 2>&1; then
    exec qemu-system-arm "$@"
fi

if ! docker image inspect oee-qemu >/dev/null 2>&1; then
    echo "== building the oee-qemu image (one-time; apt inside a container)" >&2
    repo="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
    docker build -t oee-qemu "$repo/qemu"
fi

pwd="$(pwd)"
exec docker run --rm -i -v "$pwd:$pwd" -w "$pwd" oee-qemu "$@"
