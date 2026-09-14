# Repeated SUBSCRIBE creates a duplicate subscription

- **Severity**: important
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Important #6
- **Location**: `mqtt-min/src/testing.rs:154-162`

## Summary

A `push` without dedup by `(conn_id, filter)`; per the MQTT 3.1.1 spec (§3.8.4) a new subscription replaces the existing one. The consequence is silent duplicate QoS-0 deliveries into bench data.

The probability is low (current consumers subscribe once per connection), but the fix is trivial.

## Suggested fix

`retain` + `push` (or a check before insertion).
