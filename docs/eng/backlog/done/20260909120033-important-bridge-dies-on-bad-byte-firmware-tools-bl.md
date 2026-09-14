# One corrupt serial byte kills the bridge without end markers

- **Severity**: important
- **Component**: firmware-tools
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, Important #5
- **Location**: `firmware/tools/src/bridge.rs:82-83, 84-95`

## Summary

`input.lines()` (UTF-8 validation) + `line?`: a glitch on `/dev/ttyUSB*` (wrong baud, noise) → `InvalidData` → `run_bridge` exits with an error, `oee/line1/{node}/end` is never published — and the week-5 aggregator waits for it to flush, so the whole loop hangs (see `20260909120002-critical-hang-on-lost-end-marker-oee-aggregator-bl.md`). An MQTT publish error (`bridge.rs:86-87`) breaks the loop the same way.

## Suggested fix

- Read with `read_until(b'\n')` + `String::from_utf8_lossy`;
- in the error path, best-effort publish end markers for the nodes already seen.

Also (from A6 "Minor"): `bridge.rs:46` — the `state` field reaches the JSON unvalidated: a corrupt line like `a,x,1,ab"c` injects invalid JSON into the aggregator. Restrict with a whitelist of state names (`idle|run|jam|overload` etc.).
