# Сценарий soak: ~100 000 сообщений через стенд

> [`scenarios/soak.toml`](../../scenarios/soak.toml) — сценарий под
> нагрузку сообщениями (неделя 6): те же плотности, что в
> [`scenarios/week5/normal.toml`](../../scenarios/week5/normal.toml),
> растянутые до 3 часов симулированного времени. Это soak-тест стендового
> брокера и агрегатора, а не демо-сценарий. Все числа ниже — из
> проверенного прогона (сид 42).

## Что ожидать (измерено, сид 42)

| Метрика                                        | Значение                                        |
| ---------------------------------------------- | ----------------------------------------------- |
| сообщений на `oee/line1/#` (счётчик дашборда)  | 99 787                                          |
| публикаций узлов A / P / Q                     | 9 / 24 241 / 25 465                             |
| агрегатор                                      | 49 712 принял, 50 072 переиздал, 0 ошибок       |
| минутных окон                                  | 180 (+ shift-скоп; 361 строка CSV)              |
| OEE за 3 ч                                     | 0.853 (A 0.943, P 0.952, Q 0.950), 24 239 деталей |
| генерация потоков                              | 25 с                                            |
| весь реплей, release-бинарники                 | 13 с                                            |
| артефакты                                      | ~585 МБ (`run.csv` ~337 МБ, `taps.csv` ~248 МБ) |

События состояний станка раскинуты по всей шкале намеренно: узел A
публикует только при подтверждённой смене статуса, и сгруппированные
события заставили бы его молчать часами (P и Q стримят независимо — они и
доминируют в счёте).

## Запуск 1: одной командой (debug, медленный реплей)

```bash
scripts/bench.sh scenarios/soak.toml 42
```

Скрипт сгенерирует потоки (~585 МБ в `tmp/bench/`), поднимет брокера и
агрегатор, откроет дашборд и запустит узлы в фоне. Счётчик сообщений
намотает ~99 800, затем «stream ended»; гейджи замрут на 3-часовых
значениях (OEE 85.3%, A 94.3%, P 95.2%, Q 95.0%, деталей 24 239). Выход —
`q`.

Нюанс: `bench.sh` собирает **debug**-бинари (быстрые пересборки в
демо-масштабе) — на таком размере CSV реплей идёт ~7–10 минут.

## Запуск 2: быстро (release, реплей ~13 с)

```bash
cargo build -q --release -p mqtt-min -p nodes -p oee-aggregator -p oee-dashboard

PORT=18845; ADDR=127.0.0.1:$PORT; OUT=tmp/soak
./target/release/broker $PORT >"$OUT/broker.log" 2>&1 &
./target/release/aggregator --mqtt $ADDR --ideal-cycle-ms 400 \
    --out "$OUT/oee_windows.csv" >"$OUT/aggregator.log" 2>&1 &
sleep 0.5
( sleep 1
  ./target/release/node --kind a --input "$OUT/run.csv"   --offline "$OUT/statuses.csv" --mqtt $ADDR --run-id soak-42 >"$OUT/a.log" 2>&1
  ./target/release/node --kind p --input "$OUT/ir.csv"    --offline "$OUT/counts.csv"   --mqtt $ADDR --run-id soak-42 >"$OUT/p.log" 2>&1
  ./target/release/node --kind q --input "$OUT/taps.csv"  --meta "$OUT/taps_meta.csv" \
      --offline "$OUT/verdicts.csv" --mqtt $ADDR --run-id soak-42 >"$OUT/q.log" 2>&1
) &
./target/release/oee-dashboard --mqtt $ADDR
```

Секунда форы (`sleep 1`) даёт дашборду подписаться до первой публикации
(у брокера нет ретенций). После `q` на дашборде остановите фоновые job'ы:

```bash
kill %1 %2
```

Для полностью чистого прогона сперва перегенерируйте потоки (release-
симулятор — считанные секунды):

```bash
mkdir -p tmp/soak
./target/release/line-simulator --scenario scenarios/soak.toml --seed 42 \
    --out tmp/soak/run.csv --taps-dataset tmp/soak/taps.csv --taps-meta tmp/soak/taps_meta.csv \
    --belt-events tmp/soak/ir.csv --belt-meta tmp/soak/belt_meta.csv
```

## Уборка

Артефакты soak весят ~585 МБ на выходной каталог:

```bash
rm -rf tmp/soak tmp/bench
```

## Почему debug здесь медленный

Узел A парсит 337 МБ CSV из 135 000 окон: 184 с в debug против 6 с в
release — сам инференс пренебрежим (135 000 × ~15 мкс ≈ 2 с). В
демо-масштабе (60-секундные сценарии) debug — правильный компромисс; в
soak-масштабе используйте release.
