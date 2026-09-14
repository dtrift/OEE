# Node A (firmware): `t_ms` is always 0 due to integer division

- **Severity**: critical
- **Component**: firmware-a
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, Critical #2
- **Location**: `firmware/a/src/main.rs:83` (`const SAMPLE_US: u32 = 625` — L42)

## Summary

`t_ms = t_ms.wrapping_add(SAMPLE_US / 1000)` — `625 / 1000 == 0` in integer division, so the timestamps never advance. All status lines go out as `a,bench-a,0,<state>` — the S3 cross-check (board vs host by `t_ms`) and the aggregator's time-based window logic break silently.

## Suggested fix

Accumulate microseconds (`t_us: u64`; u32 would overflow after ~71.6 min) and divide at formatting time:

```rust
format_status(..., (t_us / 1000) as u32, ...)
```

## Tests

A host test for `t_ms` monotonicity in the statuses.
