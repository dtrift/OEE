# The `qemu/` package is not covered by CI at all

- **Severity**: important
- **Component**: ci
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, Important #3
- **Location**: `.github/workflows/ci.yml` (4 jobs: `workspace`, `fork`, `firmware`, `firmware-host`)

## Summary

No job builds `qemu/`, runs fmt/clippy on it, or executes `scripts/qemu-parity.sh` — the week-6 gate ("PARITY OK") exists only as a manual command. A regression (model retraining, window regeneration, a `cortex-m`/nalgebra-patch bump, a fmt break) passes CI unnoticed.

Two adjacent gaps of the `firmware` job (`ci.yml:91-95, 100-106`):

- `cargo install espup` without a version pin — toolchain drift;
- a debug build without `--release` — release-profile LTO issues are not caught, although README recommends flashing release.

## Suggested fix

- A new `qemu` job: `rustup target add thumbv7m-none-eabi`, `cargo install flip-link`, `cargo fmt/clippy/build` in `qemu/`, `apt install qemu-system-arm`, then `timeout 300 scripts/qemu-parity.sh` (the timeout is mandatory: a firmware panic = `panic-halt` = infinite loop = a hung QEMU).
- `--release` in the `firmware` job; pin the espup version.
