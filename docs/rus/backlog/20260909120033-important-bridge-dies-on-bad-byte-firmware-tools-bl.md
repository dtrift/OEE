# Один битый байт из serial убивает мост без end-маркеров

- **Серьёзность**: важно
- **Компонент**: firmware-tools
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А6, «Важно» №5
- **Локация**: `firmware/tools/src/bridge.rs:82-83, 84-95`

## Суть

`input.lines()` (UTF-8 валидация) + `line?`: глитч на `/dev/ttyUSB*` (неверный baud, шум) → `InvalidData` → `run_bridge` выходит по ошибке, `oee/line1/{node}/end` не публикуются — а week-5 агрегатор ждёт их для flush'а, т.е. виснет вся петля (см. `20260909120002-critical-hang-on-lost-end-marker-oee-aggregator-bl.md`). Ошибка MQTT publish (`bridge.rs:86-87`) прерывает цикл так же.

## Предлагаемое исправление

- Читать `read_until(b'\n')` + `String::from_utf8_lossy`;
- в error-пути перед выходом best-effort опубликовать end-маркеры уже виденных нод.

Сюда же (из «Мелочей» А6): `bridge.rs:46` — поле `state` без валидации попадает в JSON: битая строка вида `a,x,1,ab"c` инжектит невалидный JSON агрегатору. Ограничить whitelist'ом имён состояний (`idle|run|jam|overload` и т.п.).
