# Нет connect/write таймаутов

- **Серьёзность**: важно
- **Компонент**: mqtt-min
- **Источник**: ревью `backlog/20260909120000-review.md`, приложение А4, «Важно» №5
- **Локация**: `mqtt-min/src/lib.rs:81` (блокирующий `TcpStream::connect`), `write_all` без `set_write_timeout`

## Суть

- Блокирующий `connect` на нерутабельном адресе — минуты SYN-ретраев: контракт `MqttSink` «publishing must never kill the node» деградирует до зависания ноды.
- `write_all` без write-таймаута зависает навсегда при переполненном TCP-окне медленного брокера.

На localhost бенча не стреляет, но это дешёвая страховка.

## Предлагаемое исправление

`TcpStream::connect_timeout` + `set_write_timeout`.
