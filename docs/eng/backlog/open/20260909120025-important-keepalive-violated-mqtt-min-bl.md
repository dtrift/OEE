# Keepalive contract violated on both sides

- **Severity**: important
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Important #4
- **Location**: `mqtt-min/src/lib.rs` (client: keepalive sent, no pings), `testing.rs:97` (broker: 30-second read timeout); consumer: `oee-dashboard/src/mqtt.rs` (`send_ping` never called)

## Summary

The client accepts `keepalive_s` and sends it in CONNECT but never pings automatically; the consumers do not ping either. Meanwhile the broker kills idle connections on a 30-second read timeout: in quiet periods the broker side drops dashboard connections (against a real mosquitto the same happens at ~90 s). Reconnect loops mask the problem at the cost of losing QoS-0 messages during the reconnect window.

## Suggested fix

- Auto-keepalive in `next_message` on a deadline derived from `keepalive_s`;
- or broker-side keepalive of `1.5×` from CONNECT instead of a flat 30 s;
- at minimum — document the obligation to ping.
