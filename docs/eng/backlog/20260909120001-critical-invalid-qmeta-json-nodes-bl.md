# Invalid JSON in `publish_q_meta`

- **Severity**: critical
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A2, Critical #1; independently confirmed by the aggregator reviewer (A3)
- **Location**: `nodes/src/mqtt_sink.rs:124-129`

## Summary

In the raw string `r#"{{\"model\":...}}"#` the sequence `\"` is a literal backslash + quote (raw strings in Rust have no backslash escaping). The resulting payload of the `oee/line1/q/meta` topic:

```
{\"model\":\"model_q.tflite\",...}
```

No JSON parser (including the aggregator's) will accept it. For comparison: `publish_a_meta` (`mqtt_sink.rs:116-121`) is written correctly.

## Why it matters

- Nobody parses `q/meta` today, but for the first consumer (dashboard, aggregator, logger) this is a delayed-action landmine.
- The bug slipped past the tests: `statuses_reach_the_broker_with_the_pinned_shapes` checks only meta A, `publish_p_meta` has its own test, `publish_q_meta` is not covered at all.

## Suggested fix

Remove `\` from the raw string — make the format identical to `publish_a_meta`.

## Tests

Add a payload test for `publish_q_meta` (would have caught this bug).
