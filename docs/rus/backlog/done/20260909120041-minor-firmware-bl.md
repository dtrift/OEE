# Мелочи: firmware

- **Серьёзность**: мелочь
- **Компонент**: firmware
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А6, «Мелочи»

Сводный список мелких замечаний:

1. DRY: `Cursor` скопирован 3 раза (`firmware/a/src/lib.rs:189-216`, `q/src/lib.rs:79-105`, `p/src/lib.rs:65-92`). Вынести в общий крейт (модуль в `board` или отдельный `fmt-util`).
2. DRY: argmax-сниппет повторён 4 раза (`qemu/src/lib.rs:44-50`, `qemu/src/bin/dense.rs:39-44`, `firmware/a/src/lib.rs:47-53`, `firmware/q/src/lib.rs:38-44`). Хотя бы общий helper в `firmware-a`/`firmware-q`.
3. `firmware/a/src/lib.rs:175-185` — `format_capture` не используется целевым кодом (только тест). Пометить как capture-режим в NOTES/README либо убрать до появления потребителя.
4. `firmware/q/src/lib.rs:131-146` — тест `classify_matches_the_host_node_q` фактически vacuous (`verdict < 2` всегда true), что честно признано в комментарии; но `ml/models/model_q.val.csv` существует — можно закрепить реальное окно, как сделано в `firmware-a`.
