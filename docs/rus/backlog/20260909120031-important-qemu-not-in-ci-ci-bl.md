# Пакет `qemu/` не покрыт CI вообще

- **Серьёзность**: важно
- **Компонент**: ci
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А6, «Важно» №3
- **Локация**: `.github/workflows/ci.yml` (4 job'ы: `workspace`, `fork`, `firmware`, `firmware-host`)

## Суть

Ни одна job'а не собирает `qemu/`, не гоняет fmt/clippy и не запускает `scripts/qemu-parity.sh` — гейт недели 6 («PARITY OK») существует только как ручная команда. Регрессия (переобучение модели, регенерация окон, bump `cortex-m`/nalgebra-патча, поломка fmt) пройдёт CI незаметно.

Сюда же два смежных пробела job'ы `firmware` (`ci.yml:91-95, 100-106`):

- `cargo install espup` без пиннинга версии — дрейф инструмента/тулчейна;
- сборка debug без `--release` — LTO-проблемы релизного профиля не ловятся, хотя README советует прошивать release.

## Предлагаемое исправление

- Новая job `qemu`: `rustup target add thumbv7m-none-eabi`, `cargo install flip-link`, `cargo fmt/clippy/build` в `qemu/`, `apt install qemu-system-arm`, затем `timeout 300 scripts/qemu-parity.sh` (timeout обязателен: panic в прошивке = `panic-halt` = вечный цикл = зависший QEMU).
- `--release` в job `firmware`; пин версии espup.
