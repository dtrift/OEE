# OEE-стенд на TinyML: цифровой двойник

Цифровой двойник производственной линии: детерминированный симулятор → узлы
TinyML (форк [microflow-rs](./fork/microflow) с `Conv1D`) → OEE-агрегатор.
Полный план — `kontext/20260820-074824-OEE.md` (не в git).

## Структура

| Путь              | Назначение                                                      |
| ----------------- | --------------------------------------------------------------- |
| `line-simulator/` | FSM станка + синтез сигнала тока + CSV (датасет и ground truth) |
| `nodes/`          | Узлы A (ток) / P (счёт) / Q (акустика) — недели 4–5             |
| `oee-aggregator/` | A × P × Q → OEE — неделя 5                                      |
| `features-cli/`   | Общий Rust-код фич (parity обучения и инференса) — недели 3–4   |
| `fork/microflow`  | Форк движка microflow-rs (Conv1D) — свой workspace              |
| `ml/`             | Python: Keras → int8 tflite + дампы                             |
| `scenarios/`      | Декларативные TOML-сценарии прогонов (ground truth)             |
| `spike/`          | Спайк-доки недели 1 (сериализация Conv1D)                       |

## Сборка и тесты

```bash
cargo build && cargo test && cargo clippy --all-targets -- -D warnings
```

Для сборки нужен патч nalgebra из git — уже продублирован в корневом
`Cargo.toml` (`[patch.crates-io]`).

## Симулятор

```bash
cargo run -p line-simulator -- --scenario scenarios/base.toml --seed 42 --out run1.csv
```

Выход: CSV `t_ms,current_a,state` (state — истинный режим, ground truth).
Детерминизм: один seed → побитово одинаковый CSV (тест `deterministic_csv`).

## Python (ML-скрипты)

TensorFlow нужен Python 3.12 (системный 3.14 не поддерживается TF).
Окружение живёт в `tmp/` (gitignored):

```bash
tmp/venv312/bin/python ml/scripts/build_conv1d_model.py   # спайк-модель + дамп
tmp/venv312/bin/python ml/scripts/build_dense_model.py    # dense-бонус
```

## Форк microflow

`fork/microflow` — клон https://github.com/matteocarnelos/microflow-rs
(коммит `6d193da`). Сборка и тесты:

```bash
cd fork/microflow && cargo test
cargo run --example sine        # predict() на хосте
cargo run --example dense_spike # наша Keras-модель через #[model]
```

Документы: `fork/NOTES.md` (структура), `fork/docs/conv1d-spec.md` (спека
Conv1D — контракт недель 2–3).

Подключение форка: пока обычный git-клон (спайк недели 1); при первом коммите
скелета переводится на git submodule — история форка нужна для будущего PR в
апстрим, путь `fork/microflow` не меняется (path-зависимости не затронуты).

## Карта недели 1

- Д1: форк собирается, `cargo test` зелёный, `sine` предсказывает — риск №1 снят.
- Д3: факт сериализации Conv1D зафиксирован — `spike/conv1d-serialization.md`.
- Д4: спека Conv1D — `fork/docs/conv1d-spec.md`.
- Д5: этот каркас workspace.
- Д6: симулятор (FSM + сигнал + детерминизм).
- Д7: гейт — `tmp/OEE/week1-gate.md` (не в git).
