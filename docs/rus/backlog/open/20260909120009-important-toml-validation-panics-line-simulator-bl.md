# Паники из пользовательского TOML в горячем цикле

- **Серьёзность**: важно
- **Компонент**: line-simulator
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А1, «Важно» №2
- **Локация**: `signal.rs:56, 67`, `taps.rs:118-119, 127`; отсутствие валидации: `scenario.rs:170-175` (`Noise`), `scenario.rs:204-222` (`Envelope`), частичная `Taps::validate` (`scenario.rs:65-91`)

## Суть

`Noise.sigma_a`, `Signal.drift_sigma`, `Taps.noise_sigma`/`crack_noise_boost` не валидируются вовсе:

- `noise.sigma_a = -0.1` (или `nan`/`inf` — TOML их парсит) проходит парсинг и падает на первом же сэмпле через `.expect("sigma >= 0")` (`signal.rs:67`);
- отрицательный `amp_jitter`/`freq_jitter` даёт пустой диапазон `random_range(0.2..=-0.2)` → паника внутри `synth_window`;
- отрицательные `envelope.*` молча принимаются (сигнал переворачивается по знаку).

Это паттерн «raise-in-loop убивает весь batch», которого проект старается избегать. `belt` валидируется образцово, а соседние секции — нет; асимметрия сама по себе источник ошибок.

## Предлагаемое исправление

Добавить в `Scenario::parse` (`scenario.rs:266-272`) валидацию по образцу `Belt::validate`:

- `sigma_a`/`drift_sigma`/`noise_sigma` — конечные и ≥ 0;
- `amp_jitter`/`freq_jitter` ∈ [0, 1);
- `crack_noise_boost` ≥ 0;
- `envelope.*` ≥ 0;
- `signal.third`/`fifth` — конечные.

После этого `expect` останутся недостижимыми — заменить на `unwrap_or`-ветку или оставить с комментарием «validated in parse».
