# End marker is published without retry and not on all exit paths

- **Severity**: important
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A2, Important #1
- **Location**: `nodes/src/bin/node.rs:89, 121, 157`; related: `nodes/src/mqtt_sink.rs:139-142`, `a.rs:56`, `q.rs:43`

## Summary

- If the node panics before `publish_end` (e.g. `partial_cmp().unwrap()` in `a.rs:56`/`q.rs:43` on NaN, or the process is killed), the `{node}/end` marker never goes out — the aggregator waiting for all nodes hangs.
- Even on a clean shutdown, `publish_end` is **one attempt**: if the broker is unavailable at that moment (the D5 degradation scenario), the marker is silently lost (the returned `bool` is ignored).
- The results of `publish_{a,p,q}_meta` (lines 83, 118, 154) are ignored the same way: broker unavailable at startup → meta lost forever, even though statuses later reconnect.

The paired card on the aggregator side: `20260909120002-critical-hang-on-lost-end-marker-oee-aggregator-bl.md` (timeout/deadline).

## Suggested fix

- Retry `publish_end` with a capped backoff N times (it is a flush signal, more important than regular statuses).
- At minimum, log/account for meta publication failures.
- `catch_unwind` around `run_*` publishing end before re-panicking.

## Tests

- End markers for A and Q (currently only P is covered: `node_p_publishes_counts_over_mqtt`; `both_nodes_one_run_with_mqtt` publishes neither meta nor end).
- `MqttSink` reconnect when the broker dies between publishes (currently only a dead broker is tested).
