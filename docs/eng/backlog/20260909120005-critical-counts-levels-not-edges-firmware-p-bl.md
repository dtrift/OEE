# Node P (firmware): counts levels instead of edges — part overcount

- **Severity**: critical
- **Component**: firmware-p
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, Critical #1
- **Location**: `firmware/p/src/lib.rs:35-50`; host reference: `nodes/src/p.rs`

## Summary

`observe(level=true)` counts a part on *every* poll outside the 50 ms window: `!level → return`, but there is no low→high transition check inside. A pin held high longer than 50 ms (a real part in the barrier gap — tens/hundreds of ms at a 400 ms feed rate) produces a repeated count every ~51 ms. Even the project's own check from `firmware/README.md` ("jumper GPIO5 → 3V3, hold ≥ 60 ms") yields a double count.

Additionally, the semantics diverged from the host reference `nodes/src/p.rs`:

- the host counts only rising edges (0→1, there is `last_level`);
- the host's anti-double window is anchored to the last *counted* event (`last_counted`, not extended), while the firmware uses a sliding window (`last_edge_ms` extended by every bounce, `<=` instead of `<`).

The test `low_levels_never_count` exists; there is no test for a held-high level.

## Why it matters

Node P is the Performance metric: a systematic part overcount.

## Suggested fix

- Store `prev_level`, count only on `level && !prev_level`.
- Anchor the window to the last counted event (`t_ms.wrapping_sub(last) < debounce_ms`), as the host does.
- Test "held high 200 ms → exactly 1".
- After the fix: a property-style trace `bounce/held-high/wrap` against the host `nodes::p::EdgeCounter` on identical inputs (the declared "mirroring").
