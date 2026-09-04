# firmware/ — прошивки узлов (колея железа)

Английская версия: [README.md](README.md). План шейкдауна (сессии S0–S7,
гейт): [docs/rus/../eng/decompose/firmware.md](../docs/eng/decompose/firmware.md).

Прошивки ESP32-S3 для узлов A/P/Q. Стенд: 2× ESP32-S3-DevKitC-1 (N16R8) —
узлы A и Q; 1× ESP32-S3-WROOM-1 N16R8 **CAM** с OV2640 на борту — узел P и
стретч-камера (обвязка камеры занимает часть пинов). Отдельный workspace,
как `fork/microflow`: целевой тулчейн (Xtensa, `espup`) не должен влиять
на хостовый CI корневого workspace.

## Статус: реализовано, ожидает шейкдауна на стенде

Колея реализована в коде; физический подъём (S0 blinky → S6 счёт) —
оставшаяся человеческая часть на железе:

- **`firmware-{a,q,p}`** собираются под `xtensa-esp32s3-none-elf`
  (esp-hal 1.2, драйверные модули `unstable`) **и** на хосте (бинарь —
  пустая заглушка; логика узла — хост-тестируемая lib каждого крейта):
  - `a`: ADC1@GPIO4 1.6 кГц → стартовая калибровка нуля
    (`with_zero_counts`) → окно 128 → `model_a` → гистерезис ×2 → строки
    `a,run_id,t_ms,state`;
  - `q`: серво-PWM (LEDC 50 Гц, GPIO11) → пауза → окно 1024 → `model_q` →
    строки `q,run_id,t_ms,verdict`. **S4 TODO**: окно — синтетический тап,
    пока не встал драйвер I2S/INMP441 (`i2s_slots_to_f32` в lib закреплён
    хостовыми тестами; проверка тона с дампом — шаг шейкдауна);
  - `p`: TCRT5000@GPIO5, опрос 1 кГц → антидребезг 50 мс → счётчик →
    строки `p,run_id,t_ms,count`.
- **`firmware-tools`** (хост): `capture-to-run` (capture-CSV платы → вход
  хостового узла, кросс-чек S3) и `uart-bridge` (строки из stdin → MQTT
  `oee/line1/*` + end-маркеры — фаза 2: цикл недели 5 против физического
  стенда без строчки сети на платах).
- **CI**: два job'а — `firmware` (Xtensa, только сборка) и
  `firmware-host` (fmt + clippy + тесты lib).

## Сборка

Хост (тулчейн не нужен — тесты логики):

```bash
cd firmware && cargo test --workspace
```

Цель (один раз на шелл):

```bash
cargo install espup && espup install   # пропатченный Xtensa-тулчейн
. $HOME/export-esp.sh                  # PATH линкера (xtensa-esp-elf-gcc)
```

Esp-тулчейн стоит в `~/.rustup/toolchains/esp` (дефолт espup), который
asdf-шелловый rustup не видит — поэтому полный путь:

```bash
cd firmware
~/.rustup/toolchains/esp/bin/cargo build \
    -p firmware-a -p firmware-q -p firmware-p \
    --target xtensa-esp32s3-none-elf
```

Прошивка и монитор — см. следующий раздел.

## Прошивка плат

Один ELF на плату (espflash сам превращает его в загрузочный образ —
отдельный .bin готовить не нужно). Берите **release**-сборки; бинарник
должен соответствовать обвязке платы (`board` — единый источник правды):

| Плата                                  | Узел | Файл (`firmware/target/xtensa-esp32s3-none-elf/release/`) |
| -------------------------------------- | ---- | ---------------------------------------------------------- |
| DevKitC-1 №1 (ACS712 на GPIO4)         | A    | `firmware-a`                                               |
| DevKitC-1 №2 (серво GPIO11 + INMP441)  | Q    | `firmware-q`                                               |
| CAM-плата (TCRT5000 на GPIO5)          | P    | `firmware-p`                                               |

Платы различаются по USB-последовательному порту — смотрите `/dev/ttyUSB*`:

```bash
espflash flash --port /dev/ttyUSB0 target/xtensa-esp32s3-none-elf/release/firmware-a
espflash flash --port /dev/ttyUSB1 target/xtensa-esp32s3-none-elf/release/firmware-q
espflash flash --port /dev/ttyUSB2 target/xtensa-esp32s3-none-elf/release/firmware-p
```

`--monitor` сразу после прошивки покажет консоль. Если плата не видится —
зажмите **BOOT** при подключении (режим прошивки по USB).

Что печатает живой образ в UART (115200) — первая проверка шейкдауна:

- A: `a: boot, run_id=bench-a` → `a: zero=NNN` (стартовая калибровка
  нуля) → строки `a,bench-a,<t_ms>,<state>` при подтверждённых сменах;
- Q: `q: boot, run_id=bench-q` → `q,bench-q,<t_ms>,<verdict>` каждые
  ~400 мс (окно синтетическое до шага S4 с I2S);
- P: `p: boot, run_id=bench-p` → `p,bench-p,<t_ms>,<count>` на деталь.

Прошивать НЕ надо:

- `qemu/target/.../oee-qemu` — артефакт недели 6 под LM3S6965/Cortex-M3,
  другой тулчейн и другая цель; на ESP32-S3 он не стартует;
- debug-сборки работают, но в 10 раз больше без пользы для шейкдауна
  (паники печатают строку в UART и в release).

## Контракты в игре

- `board` — пины стенда, единый источник правды (тест сверяет назначения
  с reserved-списком S3);
- `features-cli` (корневой workspace) — контракты окна/частоты
  (`window_spec`), калибровка ADC → амперы (включая коррекцию нуля в
  рантайме), схема capture-CSV; `#![no_std]`, подключён path-зависимостью;
- `nodes::source::SensorSource` — контракт источника данных узла (близнец
  хостового `SimSource`); lib прошивок зеркалят семантику хостового
  `nodes::status` (окно/гистерезис), закреплено тестами, включая фикстуру
  паритета модели на реальном валидационном окне.

## Крейты

| Крейт           | Роль                                                                |
| --------------- | ------------------------------------------------------------------- |
| `board`         | Пины стенда по узлам + reserved (N16R8; CAM-плата — узел P)          |
| `firmware-a`    | Узел A: ACS712 → ADC1 → калибровка → окно → predict → статус        |
| `firmware-q`    | Узел Q: серво-тапер → I2S INMP441 → окно → predict → вердикт        |
| `firmware-p`    | Узел P (CAM-плата): TCRT5000 → фронт + антидребезг 50 мс → счёт     |
| `firmware-tools`| Хостовые инструменты: `capture-to-run`, `uart-bridge`               |

Питание серво узла Q — отдельные 5 В (не USB платы): бросок тока серво
просаживает шину и ребутит плату; держите конденсатор 470 мкФ у пинов
серво.

## Цикл стенда (фаза 2, после шейкдауна)

```bash
# терминал 1: брокер + агрегатор/дашборд недели 5
./target/debug/broker 1883 &
./target/debug/aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 --out windows.csv
./target/debug/oee-dashboard --mqtt 127.0.0.1:1883
# терминал 2: строки плат в MQTT
cat /dev/ttyUSB0 | cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```
