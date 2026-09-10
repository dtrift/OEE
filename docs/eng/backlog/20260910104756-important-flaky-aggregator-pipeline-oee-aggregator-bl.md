# Flaky test `aggregator_subscribes_folds_publishes_and_flushes_on_end_markers`

- **Severity**: important
- **Component**: oee-aggregator
- **Source**: a local run of all CI checks on 2026-09-10 (not from the code review — found during verification)
- **Location**: `oee-aggregator/tests/aggregator_pipeline.rs:32-147`; the broker: `mqtt-min/src/testing.rs` (`LoopbackBroker::spawn` → `bind("127.0.0.1:0")`)

## Symptom

Under `cargo test --workspace` (suites running in parallel) the test failed **once in 4 runs** (~25%):

- the failed run: the test duration was **0.04 s** — an instant error, not a timeout (the test uses 5 s timeouts on `next_oee`/`recv_timeout` and a 30 s deadline);
- the isolated run `cargo test -p oee-aggregator --test aggregator_pipeline` — stably green;
- 3 subsequent full `cargo test --workspace` runs — green (0 failures).

The exact panic message was **not captured** — the first run's output was filtered through a grep for `test result`. Capturing it is the first diagnostics step (below).

## Repro

```bash
cd /home/timur/projects/OEE

# isolated — expected stably green (the control):
cargo test -p oee-aggregator --test aggregator_pipeline

# full parallel run in a loop — catch a failure WITH the full output:
for i in $(seq 1 20); do
  echo "== run $i";
  cargo test --workspace 2>&1 | tee /tmp/ws-test-$i.log | grep -E "FAILED|panicked";
done

# the parallelism hypothesis check (sequential suites):
cargo test --workspace -- --test-threads=1
```

Expectation: failures only in the parallel mode; the failed run's `/tmp/ws-test-*.log` shows the full panic (the test line + the cause).

## What is already ruled out

- **A port conflict between suites**: `LoopbackBroker::spawn` binds an ephemeral port (`127.0.0.1:0`) — the parallel mqtt-min/aggregator suites share no ports.
- **The recent changes** (plan 20260910095045 and the 07/31/47 follow-up): the aggregator and its tests were not touched; the failure reproduces on code that was green in CI before.

## Hypotheses (most plausible first)

1. **A thread race on the wire path under load**: the test creates 5 connections (dashboard, aggregator, 3 nodes) to a broker with a thread per connection; under the parallel load of other crates' test threads, instant `Err`s on `Client::connect(...).unwrap()` / `subscribe(...).unwrap()` / `next_message` (a connection reset by the broker's reader thread) are plausible.
2. **Subscription registration vs publishing**: the `ready` barrier covers the aggregator's subscription, but the `dashboard` subscription to `oee/line1/#` is not barriered by anything — if the broker's reader thread did not register it before... (less likely: the subscription happens before the nodes start).
3. **A leak/crash of the LoopbackBroker reader thread** under parallel load (the capture channel queue, a panic in a connection thread).

## Suggested fix

1. Capture the failing run's panic (the repro above) — do not fix blind before that.
2. Depending on the failing line:
   - `connect`/`subscribe` in the test — replace `.unwrap()` with a retry on a bounded backoff (a small helper), the way `MqttSink` in the nodes already does;
   - a race in the broker/client — fix in `mqtt-min` (the adjacent candidate is the reader thread in `testing.rs`);
3. If the root cause is CI machine load: do NOT `#[ignore]` — make the test resilient (a retry) instead: it is the only e2e wire-path test of the aggregator runtime.

## Related cards

- `20260909120044-rec-oee-aggregator-bl.md` — the wire-path test recommendations (extend with "parallel-load resilience");
- `resolved/20260909120002-critical-hang-on-lost-end-marker-oee-aggregator-bl.md` — the same test file: the un-timed `join()` in the neighboring `experiment.rs` was already flagged by the review.
