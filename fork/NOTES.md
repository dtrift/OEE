# NOTES — структура форка microflow-rs

> Рабочий журнал недели 1 (Д1–Д2). База: коммит `6d193da` (main), microflow v0.1.3.

## Д1 — сборка форка: факты

- Rust: 1.96.1 (зафиксирован в `.tool-versions` проекта, asdf/rustup).
- `cargo build --release`: ~10 с (cold cache, 17 с CPU).
- `cargo test --release`: 25 lib-тестов + 3 integration — все зелёные.
- Пример `sine` на хосте: `Predicted sin(0.5): 0.413` vs точное `0.479` (модель
  игрушечная, погрешность ожидаема). Риск №1 (сборка) снят.
- Нюанс сборки: форок зависит от `nalgebra` через **git-patch**
  (`[patch.crates-io] nalgebra = { git = "https://github.com/matteocarnelos/nalgebra" }`).
  Патч из манифеста **зависимости не применяется** — если наш workspace будет
  зависеть от форка по path, тот же `[patch.crates-io]` нужно продублировать
  в корневом `Cargo.toml` workspace (иначе nalgebra возьмётся с crates.io).
- Sandbox-особенность (не относится к форку): cargo вызывается через обёртку
  `tmp/bin/cargo` с `CARGO_HOME` в `tmp/cargo-home`.

## Д2 — крейты и роли

| Крейт              | Роль                                                                                                         |
| ------------------ | ------------------------------------------------------------------------------------------------------------ |
| `microflow`        | рантайм: типы тензоров, буферы, операторы (no_std, без аллокации)                                            |
| `microflow-macros` | компилятор: proc-macro `#[model]`, парсинг tflite, генерация кода                                            |
| `examples/`        | примеры: хост (`sine`, `speech`, `person_detect`) + платформы (QEMU, ESP32, Arduino), исключены из workspace |

Рантайм `microflow/src/`:

- `tensor.rs` — `Tensor2D`/`Tensor4D` (тип, форма, scale/zero-point, квант-набор);
  view с padding (`Same`/`Valid`), свёртка на уровне типов.
- `buffer.rs` — `Buffer2D`/`Buffer4D` (обёртки nalgebra, no_std-совместимые).
- `ops/` — по файлу на оператор: `conv_2d`, `depthwise_conv_2d`,
  `fully_connected`, `average_pool_2d`, `reshape`, `softmax`, `transpose`.
- `activation.rs`, `quantize.rs` — fused-активации и requant-хелперы.

Макро-крейт `microflow-macros/src/`:

- `lib.rs` — точка входа `#[model(path)]`: читает `.tflite`, парсит flatbuffers,
  генерирует `predict()` / `predict_quantized()` / `predict_inner()`.
- `ops/*.rs` — парсеры операторов: `Operator` → токены слоя (`Box<dyn ToTokens>`).
- `tensor.rs`/`buffer.rs` — токенизированные версии тензоров/буферов (веса вшиваются
  в сгенерированный код как `const`).
- `../flatbuffers/tflite_generated.rs` — сгенерированный flatbuffers-ридер схемы TFLite.

## Путь модели: `.tflite` → `predict()`

1. `#[model("models/sine.tflite")]` на struct — proc-macro **на этапе компиляции**
   читает файл и парсит flatbuffers (`root_as_model`).
2. Из subgraph 0 берутся: вход/выход (форма, тип, scale/zero-point), тензоры,
   буферы (веса), список операторов.
3. Каждый оператор → парсер своего типа → токены слоя вида:

   ```rust
   const filters_0: Tensor4D<i8, F, H, W, C, Q> = /* веса из буфера */;
   let input: Tensor4D<_, OH, OW, OC, 1> = microflow::ops::conv_2d(
       input, &filters_0, [out_scale], [out_zero_point],
       Conv2DOptions { fused_activation, view_padding, strides },
       (const_0, const_1),  // requant-константы, посчитаны в макросе
   );
   ```

4. Слои сцепляются в `predict_inner()` — без аллокаций, всё в const/стеке.
5. Публичный контракт:

   - `predict(buffer: Buffer2D/4D<f32, ...>) -> Buffer2D/4D<f32, ...>` —
     квантует вход, деквантует выход;
   - `predict_quantized(buffer: Buffer2D/4D<i8|u8, ...>) -> Buffer...<f32>` —
     вход уже квантован.

6. Макрос пишет развёрнутый код в `target/microflow-expansion.rs` — удобно
   для дебага кодегена.

## Поддерживаемое и чего не хватает для Conv1D

Поддерживаемые операторы (runtime + парсер): `FULLY_CONNECTED`,
`DEPTHWISE_CONV_2D`, `CONV_2D`, `AVERAGE_POOL_2D`, `SOFTMAX`, `RESHAPE`,
`TRANSPOSE`. Типы: int8/u8. Ранги входа/выхода: **2 и 4** (rank-1 молча
расширяется до 2 добавлением ведущей 1).

Разрывы для `Conv1D` (Keras `Conv1D` → tflite):

1. **Rank-3 вход** `(1, T, C)` — макрос abort'ит: «supported ranks are 2 and 4».
2. **RESHAPE с rank-3 выходом/входом** — парсер reshape abort'ит на рангах != 2/4;
   цепочка `Reshape → CONV_2D → Reshape` требует ранг 3 в средней точке.
3. **Свёртка с H=1 (kernel 1×k)** — `conv_2d` формально поддержит (это 4D со
   страйдом 1 по одной оси), но эффективный 1D-кернел нужен отдельный
   (int8 dot по оси времени, без вложенных циклов по фиктивной оси).
4. AVERAGE_POOL_2D для 1D-случая — та же история с фиктивной осью.

Факт сериализации (дамп Д3) — в [`spike/conv1d-serialization.md`](../spike/conv1d-serialization.md);
контракт реализации — в [`docs/conv1d-spec.md`](./docs/conv1d-spec.md).

## Неделя 2 — кернел conv_1d (факты для недели 3)

- Кернел: `microflow/src/ops/conv_1d.rs` (зарегистрирован в `ops/mod.rs`).
- **Отличие от conv_2d по дизайну**: requant-константы НЕ считаются в макросе —
  кернел берёт сырые scale'ы (вход/фильтры per-channel/выход) и сам считает
  `m(f) = (scale_x·scale_w[f])/scale_out`. Кодегену недели 3 достаточно
  пробросить scale/zp тензоров и int32-bias в acc-единицах (спека §3.1/§3.3).
- **Bias — явный параметр** `Tensor2D<i32, F, 1, F>` в acc-единицах
  (значение = round(bias_real / (scale_x·scale_w[f]))), не зашит в константы.
- **Округление**: `f32::round_ties_even` (спека), а не `roundf` как в conv_2d —
  implementations могут расходиться на ±1 квант, допуск §5.3 это покрывает.
- **Насыщение**: `T::from_superset_unchecked` в simba = Rust `as`-cast
  (усечение + насыщение) — подтверждено golden-тестами на насыщающихся кейсах.
- **Same-паддинг**: не через `TensorView` (он заполняет нулями + коррекции zp
  в conv_2d), а явной геометрией: вышедшие за границы окна не дают вклада —
  это в точности «паддинг значением zp_x» из спеки §3.2. Формула TFLite:
  `out = ceil(T/stride)`, `pad_left = pad_total/2` (лишний — справа).
- Golden: `tests/golden/conv1d.txt` — 96 кейсов (12 форм × 8 вариантов, seed 42),
  генератор `examples/golden_gen.rs` (naive-reference внутри), тест
  `tests/conv1d_golden.rs` — бит-в-бит, dispatch-таблица форм должна
  соответствовать генератору.
- dev-зависимость `rand = "0.9"` добавлена (генератор фикстур); рантайм
  остался no_std без новых зависимостей.
