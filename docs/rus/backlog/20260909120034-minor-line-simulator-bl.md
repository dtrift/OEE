# Мелочи: line-simulator

- **Серьёзность**: мелочь
- **Компонент**: line-simulator
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А1, «Мелочи»

Сводный список мелких замечаний (каждый пункт — файл:строка, суть, действие):

1. `scenario.rs:12` — `DEFAULT_DURATION_MS` не используется нигде в workspace, а `duration_ms` — обязательное поле. Мёртвый и вводящий в заблуждение код: удалить или сделать реальный `#[serde(default = ...)]`.
2. `lib.rs:53-56` — `Simulator::state()` не вызывается никем в проекте. Мёртвый pub-API.
3. `signal.rs:73-76` — `window_rms` используется только тестами своего модуля; перенести в `#[cfg(test)]`/потребителя либо пометить как утилиту.
4. События с `t_ms >= duration_ms` молча никогда не применяются (главный цикл `main.rs:67-75` заканчивается раньше). Отвергать в `Scenario::parse` с сообщением — опечатка в сценарии сейчас незаметна.
5. `main.rs:60` — ошибка парсинга теряет путь к файлу: `.with_context(|| format!("parsing {}", args.scenario.display()))`.
6. `--dataset -`, `--taps-dataset -`, `--belt-events -` не поддержаны, хотя `--out -` есть (`main.rs:123`) — асимметрия UX.
7. Две механики записи CSV: `write_raw` через `csv::Writer`, остальные — ручная склейка; `dataset.rs:49-72` и `taps.rs:139-158` почти дословно совпадают — просится общий хелпер `write_labeled_csv(windows, n_cols, writer)`.
8. `main.rs:145` — `window_len = 128` литералом против константы `taps::TAP_WINDOW` (`taps.rs:34`). Завести `pub const CURRENT_WINDOW: usize = 128;` рядом с `SAMPLE_RATE_HZ`.
9. `dataset.rs:25-28` — `assert!` в библиотечной функции; для CLI корректнее `value_parser = clap::value_parser!(usize).range(1..)` — паника невозможна в принципе.
10. `lib.rs:6` — док «CSV output "time, current, true mode"» не совпадает с фактическим заголовком `t_ms,current_a,state`.
11. Времена прихода деталей в taps строго кратны `period_ms` без джиттера (`taps.rs:86, 105`), в отличие от belt — детектор теоретически может «чититерить» по времени. Зафиксировать как осознанное решение в доке модуля.
12. `Scenario::parse` возвращает `Result<_, String>` (`scenario.rs:266`) — stringly-typed ошибки; `thiserror`-энум или `Box<dyn Error>` дал бы call-sites больше контекста.
