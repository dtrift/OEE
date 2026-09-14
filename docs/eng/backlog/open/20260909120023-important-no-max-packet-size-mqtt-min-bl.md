# No remaining-length limit on allocation

- **Severity**: important
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Important #2
- **Location**: `mqtt-min/src/testing.rs:132`, `lib.rs:203`, `lib.rs:228` (`vec![0u8; len]`)

## Summary

`len` from a 4-byte varint reaches 268,435,455 — a single 4-byte header causes an instant 256 MB allocation:

- in the broker — from a corrupt/malicious client;
- in the client — from a corrupt/malicious broker (reachable combined with `20260909120003-critical-midpacket-retry-desync-mqtt-min-bl.md`: after a desync, payload bytes are interpreted as a header).

For the loopback-test CI process this is an OOM.

## Suggested fix

A `MAX_PACKET_SIZE` constant (e.g. 1 MB — comfortably above any bench payload); on violation — an error and connection close.

Better — together with `20260909120045-rec-mqtt-min-bl.md`: extract framing (varint + full-packet reading + the limit) into one private module shared by client and broker.
