# Стек и зависимости проекта OEE — 2026-08-22

- **Источник:** анализ манифестов (`Cargo.toml`), `Cargo.lock` (root, firmware, fork) и `ml/scripts/`.
- **Границы:** форк `fork/microflow` — внешний submodule, приведён для полноты.

## Тулчейн и инфраструктура

| Инструмент | Версия / способ фиксации                           |
| ---------- | -------------------------------------------------- |
| Rust       | 1.96.1 (`.tool-versions`, пин в CI)                |
| Python     | 3.12 (venv `tmp/venv312`; 3.14 TF не поддерживает) |
| CI         | GitHub Actions (workspace + fork)                  |
| Git        | submodule: `fork/microflow`                        |

## Собственные крейты

| Workspace        | Крейты                                                                 | Зависимости                             |
| ---------------- | ---------------------------------------------------------------------- | --------------------------------------- |
| корневой (root)  | `line-simulator`, `nodes`, `oee-aggregator`, `features-cli` (`no_std`) | все — у `line-simulator`; остальные — 0 |
| `firmware/`      | `board`, `firmware-a`, `firmware-q`, `firmware-p`                      | нет                                     |
| `fork/microflow` | `microflow` (runtime), `microflow-macros` (proc-macro)                 | см. разд. «Форк»                        |

## Rust: прямые зависимости (корневой workspace)

Все зависимости — у `line-simulator` (`nodes`, `oee-aggregator`, `features-cli` — без зависимостей).

| Крейт        | Версия (lock)    | Назначение                   |
| ------------ | ---------------- | ---------------------------- |
| `rand`       | 0.9.5            | seeded RNG (StdRng)          |
| `rand_distr` | 0.5.1            | Normal (шум сигнала)         |
| `serde`      | 1.0.229 (derive) | TOML-сценарии → структуры    |
| `toml`       | 0.9.12           | парсинг сценариев            |
| `csv`        | 1.4.0            | вывод `t_ms,current_a,state` |
| `clap`       | 4.6.6 (derive)   | CLI                          |
| `anyhow`     | 1.0.104          | обработка ошибок в `main`    |

## Rust: транзитивные зависимости (весь lock — 51 пакет)

- **rand-цепочка:** `rand_chacha` 0.9.0, `rand_core` 0.9.5, `ppv-lite86` 0.2.21, `zerocopy` 0.8.56 + `zerocopy-derive`, `getrandom` 0.3.4, `cfg-if` 1.0.4, `libc` 0.2.189, `r-efi` 5.3.0, `wasip2` 1.0.4, `wit-bindgen` 0.57.1
- **rand_distr:** `num-traits` 0.2.19, `autocfg` 1.5.1, `libm` 0.2.16
- **serde:** `serde_core`, `serde_derive` (`proc-macro2` 1.0.107, `quote` 1.0.47, `syn` 2.0.119 / 3.0.3, `unicode-ident` 1.0.24)
- **toml:** `indexmap` 2.14.0 (`equivalent`, `hashbrown` 0.17.1), `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, `winnow` 0.7.15 / 1.0.4
- **csv:** `csv-core` 0.1.13, `itoa` 1.0.18, `ryu` 1.0.23, `memchr` 2.8.3
- **clap:** `clap_builder`, `clap_derive`, `clap_lex`, `heck`, `strsim`, `anstream` + `anstyle`-семейство (`anstyle-parse/query/wincon`, `colorchoice`, `is_terminal_polyfill`, `once_cell_polyfill`, `utf8parse`), `windows-sys`/`windows-link` (Windows-only)

## Python (ml/scripts)

| Пакет        | Версия | Примечание                                |
| ------------ | ------ | ----------------------------------------- |
| `tensorflow` | 2.21.0 | из спайк-доков; в requirements не запинен |
| `numpy`      | —      | версия не зафиксирована                   |

Замечание: requirements-файла для `tmp/venv312` нет — версии TF/numpy живут неявно. Для воспроизводимости ML-конвейера стоит запинить (родственно BL-15 из backlog).

## Форк `fork/microflow` (submodule, отдельный workspace)

- Прямые: `microflow-macros` 0.1 (path), `nalgebra` 0.32 (default-features off, **git-patch** `matteocarnelos/nalgebra`), `simba` 0.8, `libm` 0.2.
- dev: `csv` 1.2, `criterion` 0.5.
- Из-за git-patch'а `nalgebra` в корневом `Cargo.toml` приготовлен закомментированный `[patch.crates-io]` — понадобится с недели 3, когда крейты workspace получат path-зависимость от форка.

## Заявлено, но ещё не подключено

- MQTT-библиотека, `ratatui` (дашборд) — недели 4–5.
- `esp-hal`/espup (тулчейн Xtensa) — колея железа, при обкатке.
- QEMU LM3S6965 (`thumbv7m-none-eabi`) — неделя 6, только тул.
