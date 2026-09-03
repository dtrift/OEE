# OEE Project Stack and Dependencies — 2026-09-03

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

| Workspace        | Crates                                                                                                          | Dependencies                    |
| ---------------- | --------------------------------------------------------------------------------------------------------------- | ------------------------------- |
| root             | `line-simulator`, `nodes`, `oee-aggregator`, `features-cli` (`no_std`), `ml/exporter`, `ml/trainer`, `mqtt-min` | see "Rust: direct dependencies" |
| `firmware/`      | `board`, `firmware-a`, `firmware-q`, `firmware-p`                                                               | none                            |
| `fork/microflow` | `microflow` (runtime), `microflow-macros` (proc-macro)                                                          | see the "Fork" section          |

## Rust: direct dependencies (root workspace)

Since week 3 (the D6 bridge) and the Rust-ML track, dependencies are spread across crates:

- `line-simulator` — `rand`, `rand_distr`, `serde`, `toml`, `csv`, `clap`, `anyhow`;
- `features-cli` — `libm` (no_std transcendentals for the features, week 3, D4);
- `nodes` — `microflow` (path → the fork), `features-cli` (path), `nalgebra` (week 3, D6); since week 4 — `csv`, `clap`, `anyhow`, `mqtt-min` (path);
- `ml/exporter` — `flatbuffers`, `csv`, `rand`; dev: `microflow` (path), `nalgebra`;
- `ml/trainer` — `exporter` (path), `burn`, `csv`, `rand`, `clap`, `anyhow`;
- `mqtt-min` — no dependencies (std-only; week 4: the MQTT client instead of `rumqttc`, offline);
- `oee-aggregator` — no dependencies.

| Crate         | Version (lock)   | Purpose                       |
| ------------- | ---------------- | ----------------------------- |
| `rand`        | 0.9.5            | seeded RNG (StdRng)           |
| `rand_distr`  | 0.5.1            | Normal (signal noise)         |
| `serde`       | 1.0.229 (derive) | TOML scenarios → structs      |
| `toml`        | 0.9.12           | scenario parsing              |
| `csv`         | 1.4.0            | `t_ms,current_a,state` output |
| `clap`        | 4.6.6 (derive)   | CLI                           |
| `anyhow`      | 1.0.104          | error handling in `main`      |
| `libm`        | 0.2.16           | no_std sqrt/cos for features  |
| `nalgebra`    | 0.32.2 (git)     | the fork bridge (`nodes`)     |
| `flatbuffers` | 23.5.26          | `.tflite` writer (`exporter`) |
| `burn`        | 0.21.0           | training A (`trainer`)        |

## Rust: transitive dependencies (whole lock — 576 packages)

- **rand chain:** `rand_chacha` 0.9.0, `rand_core` 0.9.5, `ppv-lite86` 0.2.21, `zerocopy` 0.8.56 + `zerocopy-derive`, `getrandom` 0.3.4, `cfg-if` 1.0.4, `libc` 0.2.189, `r-efi` 5.3.0, `wasip2` 1.0.4, `wit-bindgen` 0.57.1
- **rand_distr:** `num-traits` 0.2.19, `autocfg` 1.5.1, `libm` 0.2.16
- **serde:** `serde_core`, `serde_derive` (`proc-macro2` 1.0.107, `quote` 1.0.47, `syn` 2.0.119 / 3.0.3, `unicode-ident` 1.0.24)
- **toml:** `indexmap` 2.14.0 (`equivalent`, `hashbrown` 0.17.1), `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, `winnow` 0.7.15 / 1.0.4
- **csv:** `csv-core` 0.1.13, `itoa` 1.0.18, `ryu` 1.0.23, `memchr` 2.8.3
- **clap:** `clap_builder`, `clap_derive`, `clap_lex`, `heck`, `strsim`, `anstream` + the `anstyle` family (`anstyle-parse/query/wincon`, `colorchoice`, `is_terminal_polyfill`, `once_cell_polyfill`, `utf8parse`), `windows-sys`/`windows-link` (Windows-only)
- **burn chain (`ml/trainer`):** the main contributor to the lock growth (51 → 576 packages) — ndarray/matrixmultiply, autodiff, derive infrastructure; hence coexisting lock versions (`rand` 0.8/0.9/0.10, `syn` 1/2/3, `toml` 0.9/1.1, etc.), see `Cargo.lock` for detail. Plus `nalgebra` 0.32.2 + `simba` 0.8.1 (the fork's git-patch, the week 3 bridge).

## Python (ml/scripts)

| Package      | Version | Note                                            |
| ------------ | ------- | ----------------------------------------------- |
| `tensorflow` | 2.21.0  | from the spike docs; not pinned in requirements |
| `numpy`      | —       | version not fixed                               |

Note: there is no requirements file for `tmp/venv312` — the TF/numpy versions live implicitly. For ML pipeline reproducibility it is worth pinning them (related to BL-15 in the backlog).

Week 3 scripts: `train_model_a.py` (training + PTQ + metrics), `dump_parity_fixtures.py` (parity fixtures), `golden_features.py` (the numpy feature reference). The node A model can be born without them — the Rust-ML track ([rust-ml-gate.md](rust-ml-gate.md)).

## The `fork/microflow` fork (submodule, separate workspace)

- Direct: `microflow-macros` 0.1 (path), `nalgebra` 0.32 (default-features off, **git-patch** `matteocarnelos/nalgebra`), `simba` 0.8, `libm` 0.2.
- dev: `csv` 1.2, `criterion` 0.5.
- Because of the `nalgebra` git-patch, the root `Cargo.toml` has had `[patch.crates-io]` enabled since week 3 (the D6 bridge, `nodes`): a `[patch]` section from a path dependency's manifest is not applied — the patch is duplicated at the root.

## Declared but not yet wired up

- `ratatui` (dashboard) — week 5 (the MQTT library arrived ahead of plan — our own, `mqtt-min`, week 4).
- `esp-hal`/espup (Xtensa toolchain) — the hardware track, at shakedown time.
- QEMU LM3S6965 (`thumbv7m-none-eabi`) — week 6, tool only.
