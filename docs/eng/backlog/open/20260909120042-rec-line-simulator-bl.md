# Recommendations: line-simulator

- **Severity**: recommendation
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, "Recommendations"

Work order and process recommendations (links point to problem cards):

1. **Integer time first** (`20260909120008-important-f32-time-degradation-line-simulator-bl.md`): it removes the root cause of three symptoms (signal staircase, `t_ms` duplicates, platform-dependent `sin`) and lifts the soak duration ceiling. Land the bit change as a separate commit with artifact regeneration and new pinned test baselines.
2. TOML validation (`20260909120009-important-toml-validation-panics-line-simulator-bl.md`) — one commit following the `Belt::validate` template: the pattern is proven, the messages are good.
3. After `Simulator::run(...)` lands — a CLI integration test for channel independence: a regression guard for any future RNG consumption changes.
4. Fix memory together: conditional stream generation + streaming writes — soak runs will fit into memory comparable to a single artifact's size.
5. Start a "time precision" doc in `line-simulator/README` (or a CHANGELOG) instead of the `soak-1m.toml` comment — the f32-ceiling knowledge is too important to live in one scenario's config.
