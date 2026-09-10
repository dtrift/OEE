# Minor: oee-dashboard

- **Severity**: minor
- **Component**: oee-dashboard
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, "Minor" (#9, 10, 15, 16)

A consolidated list of minor remarks:

1. `state.rs:159` — `history` is unbounded: verdicts are capped by `VERDICT_TICKER` (12), but the minute history grows without limit (8 bytes/minute — slow but continuous on a long multi-hour run). Cap it like the verdict ticker.
2. `main.rs:74` — exit only on `q`; Ctrl-C / Esc do not work: in raw mode Ctrl-C produces no SIGINT and is silently ignored. Add `Char('c')` with `KeyModifiers::CONTROL` and `Esc`.
3. The fixed client id `"oee-dashboard"` (`mqtt.rs:36`) — two instances on one broker conflict (mosquitto drops the older connection). Note it in the docs.
4. `state.rs:122-124` — `finished` fires on the first `{node}/end`: the "stream ended" footer lies while two other nodes are still streaming. "Stream ending" semantics is more accurate.
