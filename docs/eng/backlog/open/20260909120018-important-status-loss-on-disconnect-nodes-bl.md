# A lost node A status publish is never recovered

- **Severity**: important
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A2, Important #5
- **Location**: `nodes/src/mqtt_sink.rs:80-113`

## Summary

On connection loss the status is lost forever: the next publish reconnects but sends the already *new* change. Node P payloads carry a cumulative `count` — the aggregator recovers the gap; node A statuses are change-events, and a lost change distorts Availability until the next change.

## Suggested fix

The full solution is QoS ≥ 1 / a queue on the `mqtt-min` side (out of current scope). The cheap minimum here: one retry of the lost message after reconnection.
