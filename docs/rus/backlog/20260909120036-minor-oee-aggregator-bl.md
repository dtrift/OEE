# Мелочи: oee-aggregator

- **Серьёзность**: мелочь
- **Компонент**: oee-aggregator
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А3, «Мелочи» (№6-8, 11-15)

Сводный список мелких замечаний:

1. `experiment.rs:143-146` — мёртвый код: замыкание `artifact` тут же погашено `let _ = artifact;`. Удалить.
2. `aggregator.rs:167, 175, 183, 190` — `SOURCES.iter().position(...).unwrap()` в горячем пути. Индексы `a/p/q` известны статически: `const AT_A: usize = 0;` или `fn at(node) -> Option<usize>` без `unwrap`.
3. `payload.rs:78-85` — `find_key` аллоцирует `String` на каждый lookup, а lookup-ов несколько на сообщение (`run_id`, `t_ms`, поле payload). Искать `"` + key + `":` посимвольно или через `memchr`.
4. `bin/aggregator.rs:50-52` — `--expect` молча отбрасывает неизвестные имена: `--expect a,zz` превращается в `a` без предупреждения (опечатка = ждём не те маркеры). Падать на неизвестном имени. Аналогично `Aggregation::new` (`aggregator.rs:128`) не валидирует `expect_nodes` вовсе — опечатка в lib-вызове = тихое зависание.
5. `aggregator.rs:379-387` — `ready` сигнализируется до создания CSV: если `WindowsCsv::create` упадёт, узлы уже публикуют в никуда. Переставить создание CSV выше `ready.send`.
6. `windows.rs:73` — `stretch_from.max(from)` избыточен: `stretch_from` в этой точке всегда `>= from`. Мёртвая защита.
7. `aggregator.rs:441-443` — O(n) сканы на каждое сообщение: `shift_snapshot` публикуется после *каждого* сообщения и каждый раз сканирует все `statuses/counts/verdicts` с нуля → O(n²) за прогон. Для бенча незаметно; при масштабировании — `partition_point`/курсоры.
8. Фиксированный client id `"oee-aggregator"` (`aggregator.rs:373`): два экземпляра на одном брокере — mosquitto разорвёт старое соединение. Для бенча ок, отметить в доке (см. также `20260909120037-minor-oee-dashboard-bl.md`).
