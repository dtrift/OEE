# OEE Project Stack and Dependencies — 2026-08-22

- **Source:** analysis of the manifests (`Cargo.toml`), `Cargo.lock` (root, firmware, fork) and `ml/scripts/`.
- **Boundaries:** the `fork/microflow` fork is an external submodule, included for completeness.

## Toolchain and infrastructure

| Tool   | Version / pinning method                            |
| ------ | --------------------------------------------------- |
| Rust   | 1.96.1 (`.tool-versions`, pinned in CI)             |
| Python | 3.12 (venv `tmp/venv312`; 3.14 not supported by TF) |
| CI     | GitHub Actions (workspace + fork)                   |
| Git    | submodule: `fork/microflow`                         |

## First-party crates

| Workspace        | Crates                                                                 | Dependencies                            |
| ---------------- | ---------------------------------------------------------------------- | --------------------------------------- |
| root             | `line-simulator`, `nodes`, `oee-aggregator`, `features-cli` (`no_std`) | all — on `line-simulator`; the rest — 0 |
| `firmware/`      | `board`, `firmware-a`, `firmware-q`, `firmware-p`                      | none                                    |
| `fork/microflow` | `microflow` (runtime), `microflow-macros` (proc-macro)                 | see the "Fork" section                  |

## Rust: direct dependencies (root workspace)

All dependencies belong to `line-simulator` (`nodes`, `oee-aggregator`, `features-cli` — no dependencies).

| Crate        | Version (lock)   | Purpose                       |
| ------------ | ---------------- | ----------------------------- |
| `rand`       | 0.9.5            | seeded RNG (StdRng)           |
| `rand_distr` | 0.5.1            | Normal (signal noise)         |
| `serde`      | 1.0.229 (derive) | TOML scenarios → structs      |
| `toml`       | 0.9.12           | scenario parsing              |
| `csv`        | 1.4.0            | `t_ms,current_a,state` output |
| `clap`       | 4.6.6 (derive)   | CLI                           |
| `anyhow`     | 1.0.104          | error handling in `main`      |

## Rust: transitive dependencies (whole lock — 51 packages)

- **rand chain:** `rand_chacha` 0.9.0, `rand_core` 0.9.5, `ppv-lite86` 0.2.21, `zerocopy` 0.8.56 + `zerocopy-derive`, `getrandom` 0.3.4, `cfg-if` 1.0.4, `libc` 0.2.189, `r-efi` 5.3.0, `wasip2` 1.0.4, `wit-bindgen` 0.57.1
- **rand_distr:** `num-traits` 0.2.19, `autocfg` 1.5.1, `libm` 0.2.16
- **serde:** `serde_core`, `serde_derive` (`proc-macro2` 1.0.107, `quote` 1.0.47, `syn` 2.0.119 / 3.0.3, `unicode-ident` 1.0.24)
- **toml:** `indexmap` 2.14.0 (`equivalent`, `hashbrown` 0.17.1), `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, `winnow` 0.7.15 / 1.0.4
- **csv:** `csv-core` 0.1.13, `itoa` 1.0.18, `ryu` 1.0.23, `memchr` 2.8.3
- **clap:** `clap_builder`, `clap_derive`, `clap_lex`, `heck`, `strsim`, `anstream` + the `anstyle` family (`anstyle-parse/query/wincon`, `colorchoice`, `is_terminal_polyfill`, `once_cell_polyfill`, `utf8parse`), `windows-sys`/`windows-link` (Windows-only)

## Python (ml/scripts)

| Package      | Version | Note                                            |
| ------------ | ------- | ----------------------------------------------- |
| `tensorflow` | 2.21.0  | from the spike docs; not pinned in requirements |
| `numpy`      | —       | version not fixed                               |

Note: there is no requirements file for `tmp/venv312` — the TF/numpy versions live implicitly. For ML pipeline reproducibility it is worth pinning them (related to BL-15 in the backlog).

## The `fork/microflow` fork (submodule, separate workspace)

- Direct: `microflow-macros` 0.1 (path), `nalgebra` 0.32 (default-features off, **git-patch** `matteocarnelos/nalgebra`), `simba` 0.8, `libm` 0.2.
- dev: `csv` 1.2, `criterion` 0.5.
- Because of the `nalgebra` git-patch, the root `Cargo.toml` has a commented-out `[patch.crates-io]` section prepared — it will be needed from week 3, when the workspace crates get a path dependency on the fork.

## Declared but not yet wired up

- MQTT library, `ratatui` (dashboard) — weeks 4–5.
- `esp-hal`/espup (Xtensa toolchain) — the hardware track, at shakedown time.
- QEMU LM3S6965 (`thumbv7m-none-eabi`) — week 6, tool only.
