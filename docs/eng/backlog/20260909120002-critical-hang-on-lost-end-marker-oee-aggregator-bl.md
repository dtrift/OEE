# Aggregator hangs forever when an `end` marker is lost

- **Severity**: critical
- **Component**: oee-aggregator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, Critical #1; found independently by three reviewers (nodes, aggregator, firmware-bridge)
- **Location**: `oee-aggregator/src/aggregator.rs:393-452`; related: `nodes/src/bin/node.rs:89,121,157`, `oee-aggregator/tests/experiment.rs:228-231`

## Summary

The `run()` loop waits for `end` markers from all expected nodes. If at least one node:

- crashed (kill -9, a panic before `publish_end` — e.g. `partial_cmp().unwrap()` on NaN in `a.rs:56`/`q.rs:43`),
- lost network,
- or the single QoS 0 publication of the marker did not arrive (`publish_end` in the node is one attempt, the result is ignored),

the aggregator hangs silently: `next_message` times out, pings, waits forever. The documentation promises "mid-run disconnects abort the run", but a *node* disconnect while the broker is alive is exactly the case that does not abort.

The test harness hangs too: `experiment.rs:228-231` calls `aggregator_thread.join()` without a timeout — CI will hang instead of failing.

## Suggested fix

- A shared deadline/idle timeout in `Config`: "no new events for N seconds after at least one node has finished" → `Err` with diagnostics listing the nodes that did not finish.
- A CLI flag `--max-wait-s` with a sensible default.
- In tests: a timeout wrapper (channel + `recv_timeout` instead of `join`).
- On the node side: retry `publish_end` (separate card `20260909120015-important-end-marker-no-retry-nodes-bl.md`).

## Tests

"A node dies without `end` → the aggregator returns an error by timeout with the names of the unfinished nodes".
