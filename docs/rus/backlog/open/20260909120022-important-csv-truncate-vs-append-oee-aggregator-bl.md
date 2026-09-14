# CSV: документация обещает append, код делает truncate

- **Серьёзность**: важно
- **Компонент**: oee-aggregator
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А3, «Важно» №5
- **Локация**: `oee-aggregator/src/bin/aggregator.rs:7` (док «appends the windows CSV»), `aggregator.rs:307` (`WindowsCsv::create` → `File::create`)

## Суть

Повторный запуск с тем же `--out` стирает предыдущий прогон, хотя докоментация обещает добавление.

## Предлагаемое исправление

Либо:

- `OpenOptions::new().append(true)` с заголовком только для нового файла;
- либо честная формулировка «writes (truncates)» в доке и `--out` с уникальным именем в bench-скрипте.
