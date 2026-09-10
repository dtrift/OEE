# Mid-packet timeout retry desynchronizes the stream

- **Severity**: critical
- **Component**: mqtt-min
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A4, Critical #1
- **Location**: `mqtt-min/src/lib.rs:212-272` (`read_remaining_length_retry`, `read_exact_retry`); the invariant is declared in `lib.rs:139-142`

## Summary

`read_remaining_length_retry` calls `read_remaining_length` from scratch: if the first attempt already consumed the high varint byte (e.g. `0xC0`) and then timed out on the second byte, the retry reads the *next* stream byte as the first varint byte. Likewise `read_exact_retry`: `read_exact` on timeout has already partially filled `body` (bytes consumed from the socket), while the retry refills the buffer from index 0 — the beginning of the body is lost, the "tail" of the next packet is eaten.

The comment in `lib.rs:139-142` ("the stream cannot desync") declares exactly the opposite.

Trigger: TCP segmentation with a delay between packet parts longer than the call timeout (500 ms in the dashboard). Practically never happens on localhost, but real on a network. The consequence is silent frame corruption instead of a clean error. There is currently no buffering until a full packet.

## Suggested fix

Either:

- resumable reading: a cursor over the varint/buffer, the timeout only extends the wait under a fixed total deadline;
- or minimally: treat a mid-packet timeout as a fatal connection error (as `read_packet` already does) instead of restarting the parse.

## Tests

A test assembling a packet from TCP fragments: a fake broker writing byte-by-byte with pauses (would have covered this bug).
