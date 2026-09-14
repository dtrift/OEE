# NOTES — факты шейкдауна (колея железа)

Рабочий журнал обкатки на стенде. План сессий S0–S7 —
[docs/rus/decompose/firmware.md](../docs/rus/decompose/firmware.md); итоговый
гейт (`firmware-gate.md`, рус/англ) оформляется на S7. Пополняется по ходу
сессий.

## 2026-09-14 — S0/S1: первая прошивка, узел A

- **Плата — клон DevKitC-1**: мост USB-UART — **CH343** (QinHeng
  `1a86:55d3`), не CP2102 → порт `/dev/ttyACM0`, а не `/dev/ttyUSB*`.
  Маркировка портов «COM»/«USB» (USB-C), а не «UART»/«USB» как у v1.0 с
  micro-USB. Нативный «USB»-порт платы (Espressif `303a:4001`) тоже даёт
  `ttyACM*` — для монитора бесполезен (вывод идёт в аппаратный UART).
  Различение плат при нескольких подключённых — `/dev/serial/by-id/`
  (серийник CH343 уникален у каждой платы).
- **espflash ≥ 4.6** отказывается прошивать образ без ESP-IDF App
  Descriptor → в `firmware-{a,q,p}` добавлены target-gated зависимость
  `esp-bootloader-esp-idf = "0.6"` (feature `esp32s3`, MSRV rustc 1.95) и
  вызов `esp_bootloader_esp_idf::esp_app_desc!()` в `mod app`.
- espflash заливает **три региона**: вторичный загрузчик @0x0, таблица
  разделов @0x8000, приложение @0x10000 — образ самодостаточен, фабричный
  загрузчик в flash не нужен (загрузчик ESP-IDF v6.1-beta1 идёт в бандле
  espflash).
- Чистая машина: `cargo install espup` + `cargo install espflash`; с
  asdf-рустом бинарники попадают в `~/.asdf/installs/rust/<ver>/bin` →
  после установки `asdf reshim rust`. Права на порт — группа `dialout`
  (`sudo usermod -aG dialout $USER` + перелогин; в текущем шелле —
  `newgrp dialout`).
- **Железо узла A**: esp32s3 rev v0.2, 16 MB flash, кварц 40 МГц.
  Стартовая калибровка нуля на висящем входе ACS712: **zero=2229** отсчётов
  (середина шкалы 12-битного АЦП ≈ 2048; допуск калибровки — вопрос S1).
- Первые строки живого образа — в точности как в README: `a: boot,
  run_id=bench-a` → `a: zero=2229` → `a,bench-a,160,idle`.
- Отклонение от плана: blinky (S0) пропущен — сразу прошит рабочий
  `firmware-a`; полный журнал первой загрузки — в корневом README.
