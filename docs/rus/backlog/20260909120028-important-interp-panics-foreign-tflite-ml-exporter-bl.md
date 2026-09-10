# Interp паникует на чужих/некорректных `.tflite` вместо `Err`

- **Серьёзность**: важно
- **Компонент**: ml-exporter
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А5, «Важно» №2
- **Локация**: `ml/exporter/src/interp.rs:318, 373, 433, 447, 497` (`sx[0]`/`zpx[0]`), `interp.rs:347, 532` (`repeat(&sb[0])`), `interp.rs:77, 96, 241-242` (`unwrap` на `subgraph.inputs()`); аналогично `dumper.rs:30-36`

## Суть

- `sx[0]`/`zpx[0]` на тензоре без квантизации (например, float32-модель) → index-out-of-bounds;
- `repeat(&sb[0])` паникует при пустом векторе bias-scale;
- `.unwrap()` на `subgraph.inputs()`.

Инструмент, чья работа — читать произвольные файлы (включая TF-конвертированный), должен возвращать `Err`, а не ронять процесс.

## Предлагаемое исправление

- Хелпер `fn per_tensor_quant(t: &Tensor) -> Result<(f32, i64), String>` с проверкой длины;
- `inputs().ok_or("the subgraph declares no inputs")?`;
- то же для `dumper.rs`.
