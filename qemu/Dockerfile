# qemu-system-arm for the LM3S6965 demo (week 6, D0).
#
# The sandboxed dev box has no qemu-system-arm and no sudo; the docker image
# is the fallback path. On a normal Linux box `apt install qemu-system-arm`
# and scripts/qemu.sh will use the native binary instead — the commands and
# the output are identical.
#
# Build:  docker build -t oee-qemu qemu/
# Used by scripts/qemu.sh (the only supported entry point).
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends qemu-system-arm \
    && rm -rf /var/lib/apt/lists/*

# Version pinning happens at build time; scripts/qemu.sh prints it into the
# demo log so the report can quote the exact emulator version.
ENTRYPOINT ["qemu-system-arm"]
