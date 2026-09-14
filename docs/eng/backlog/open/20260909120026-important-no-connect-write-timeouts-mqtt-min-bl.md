# No connect/write timeouts

- **Severity**: important
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Important #5
- **Location**: `mqtt-min/src/lib.rs:81` (blocking `TcpStream::connect`), `write_all` without `set_write_timeout`

## Summary

- A blocking `connect` to an unreachable address means minutes of SYN retries: the `MqttSink` contract "publishing must never kill the node" degrades into a node hang.
- `write_all` without a write timeout hangs forever when a slow broker's TCP window fills.

Does not fire on the localhost bench, but this is cheap insurance.

## Suggested fix

`TcpStream::connect_timeout` + `set_write_timeout`.
