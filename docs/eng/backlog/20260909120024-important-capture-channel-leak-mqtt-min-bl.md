# Capture-channel leak in the bench broker on soak

- **Severity**: important
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Important #3
- **Location**: `mqtt-min/src/bin/broker.rs` (keeps `LoopbackBroker` alive), `testing.rs:189` (sends `CapturedPublish`)

## Summary

The bench binary keeps the `LoopbackBroker` (together with `publishes: Receiver`) alive and never reads it, while `testing.rs:189` sends every `CapturedPublish` (two `String`s + a channel node) into an unbounded channel. Soak ~1M messages over 12 h via `scripts/bench.sh` (the broker lives for the whole run) — roughly 150–250 MB of monotonic growth.

## Suggested fix

- Drain and discard in the binary (one `while let Ok(_)` thread);
- or make capture optional (`bind` without a channel).

While at it — periodic stats (messages, clients, disconnects) to stdout: the broker log in soak is useless today.
