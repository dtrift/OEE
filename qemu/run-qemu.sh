#!/bin/sh
# `cargo run` runner for the qemu crate: forwards to the repo-level
# qemu-system-arm wrapper (native binary or the docker fallback) from any
# working directory.
set -eu
exec "$(dirname "$0")/../scripts/qemu.sh" "$@"
