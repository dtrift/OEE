# Спайк: сериализация Keras Conv1D в .tflite (Д3, неделя 1)

> Фиксация факта по риску №2 (разд. 11 плана). Источник: TF 2.21.0, конвертер
> full-integer int8, модель-архитектура из разд. 6 плана. Ожидание плана
> (разд. 5): «Conv1D → Reshape → CONV_2D над (1, T, C)». Факт — ниже.

## TL;DR

Ожидание подтвердилось с уточнениями, значимыми для парсера:

1. Вход модели — **rank-3** `(1, T, C)` = `(1, 128, 1)`; макрос форка сейчас
   поддерживает только ранги 2 и 4.
2. Каждый Conv1D-блок — это `EXPAND_DIMS → CONV_2D → RESHAPE`, т.е. **не один
   Reshape перед CONV_2D, а пара expand/squeeze вокруг него**. `EXPAND_DIMS`
   форком не поддерживается вовсе.
3. AvgPool-блок — тот же паттерн: `EXPAND_DIMS → AVERAGE_POOL_2D → RESHAPE`.
4. Flatten перед FC — динамическая цепочка `SHAPE → STRIDED_SLICE → PACK →
   RESHAPE`: форму считает рантайм. Для компайл-тайм-парсера всё статично,
   цепочка сворачивается в один RESHAPE, но каждый op по отдельности форком
   не поддерживается.
5. Веса CONV_2D и FC — **per-channel** квантование (F scale'ов на F фильтров),
   активации — per-tensor. Runtime форка: conv_2d per-channel поддерживает,
   fully_connected — **только per-tensor** (QUANTS=1) — разрыв №2.
6. FULLY_CONNECTED **без bias**: конвертер выбрасывает нулевой bias
   (необученные Dense) и ставит optional-вход −1. Парсер форка ожидает
   bias третьим входом безусловно и паникует — разрыв №3.

## Модель

`Conv1D(8, k=3) → ReLU → AvgPool(2) → Conv1D(16, k=3) → ReLU → AvgPool(2) →
Flatten → Dense(4) → Softmax`, вход `(128, 1)`, full-int8.

Скрипт: `ml/scripts/build_conv1d_model.py`. Артефакты: `ml/models/conv1d.tflite`
(9.5 КБ), полный дамп — `ml/models/conv1d_ops.txt`; сырой дамп flatbuffers —
через probe-крейт `tmp/tflite-probe` (ридер форка, без правок форка).

## Фактический граф операторов

```text
T#0 вход (1,128,1) int8, scale=0.01282, zp=1
op[0]  EXPAND_DIMS   (T#0, axis=-3)          → T#15 (1,1,128,1)
op[1]  CONV_2D       (T#15, W(8,1,3,1), b(8)) → T#16 (1,1,126,8)   + fused RELU
op[2]  RESHAPE       (T#16, [-1,126,8])      → T#17 (1,126,8)
op[3]  EXPAND_DIMS   (T#17, axis=-3)         → T#18 (1,1,126,8)
op[4]  AVERAGE_POOL_2D (T#18)                → T#19 (1,1,63,8)
op[5]  RESHAPE       (T#19, [-1,63,8])       → T#20 (1,63,8)
op[6]  EXPAND_DIMS   (T#20, axis=-3)         → T#21 (1,1,63,8)
op[7]  CONV_2D       (T#21, W(16,1,3,8), b(16)) → T#22 (1,1,61,16)  + fused RELU
op[8]  RESHAPE       (T#22, [-1,61,16])      → T#23 (1,61,16)
op[9]  EXPAND_DIMS   (T#23, axis=1)          → T#24 (1,1,61,16)
op[10] AVERAGE_POOL_2D (T#24)                → T#25 (1,1,30,16)
op[11] RESHAPE       (T#25, [-1,30,16])      → T#26 (1,30,16)
op[12] SHAPE         (T#26)                  → T#27 [3]
op[13] STRIDED_SLICE (T#27, [0], [1], [1])   → T#28
op[14] PACK          (T#28, 480)             → T#29 [2]
op[15] RESHAPE       (T#25, T#29)            → T#30 (1,480)
op[16] FULLY_CONNECTED (T#30, W(4,480), b=-1) → T#31 (1,4)
op[17] SOFTMAX       (T#31)                  → T#32 (1,4) int8, zp=-128
```

## Layout весов и квантование

| Тензор      | Форма          | Layout / квантование                                         |
| ----------- | -------------- | ------------------------------------------------------------ |
| Conv1D веса | `(F, 1, k, C)` | OHWI: (out, h=1, w=k, in); per-channel, F scale'ов, zp=0     |
| Conv1D bias | `(F,)` int32   | per-channel: scale_b = scale_in × scale_w[f]                 |
| FC веса     | `(Out, In)`    | (out, in); per-channel по умолчанию — форку нужен per-tensor |
| FC bias     | `(Out,)` int32 | нулевой bias выбрасывается конвертером (вход −1)             |
| Активации   | любые          | per-tensor, 1 scale                                          |

Проверка layout: у собственной модели форка `person_detect.tflite` CONV_2D веса
`[16,1,1,8]` — тот же OHWI. Т.е. layout TF 2.21 совпадает с ожиданиями парсера
форка; расхождение только в ранге тензоров (rank-3 у входов/выходов блоков).

Sanity: вывод .tflite (interpreter) против Keras — max|diff| = 0.0024 на
softmax-вероятностях; argmax совпадает. Конверсия корректна.

## Бонус: полный малый цикл Keras → tflite → Rust

`ml/scripts/build_dense_model.py` собирает `Dense(16,relu) → Dense(4) →
Softmax` (вход (1,8), full-int8, **per-tensor** веса, ненулевой bias) →
`fork/microflow/models/dense_spike.tflite` → пример
`fork/microflow/examples/dense_spike.rs`:

```text
Probabilities: [0.1016, 0.1719, 0.2109, 0.5156]
Predicted class: 3
```

Попутно найдены два практических ограничения конвертера (обходы — в скрипте):

- нулевой bias у FC выбрасывается → для спайка bias инициализирован ненулевым;
- per-channel веса FC не поддержаны runtime форка → отключены флагом
  `_experimental_disable_per_channel` (для продакшн-моделей узлов это учтёт
  спека Conv1D: per-channel — обязательная часть кернела).

## Вывод для спеки (Д4)

Реальный объём работы парсера больше, чем «понять Reshape-цепочку»: нужно
сворачивать `EXPAND_DIMS`/`RESHAPE`-обёртки, статически вычислять
`SHAPE/STRIDED_SLICE/PACK`-цепочку Flatten, поддерживать rank-3 вход,
per-channel веса (FC!) и optional bias. Кернел при этом работает с уже
развёрнутым 4D-тензором (1,1,T,C) — как обычный CONV_2D с h=1.
