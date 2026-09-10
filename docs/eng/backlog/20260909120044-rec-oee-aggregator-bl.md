# Recommendations: oee-aggregator + oee-dashboard

- **Severity**: recommendation
- **Component**: oee-aggregator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, "Recommendations"

## Missing tests

- A corrupt payload over the real wire path: in `aggregator_pipeline.rs` all payloads are hand-made and valid — corrupt isolation is verified only on `Aggregation`.
- A node dies without `end` → the aggregator returns an error by timeout (after the fix from `20260909120002-critical-hang-on-lost-end-marker-oee-aggregator-bl.md`).
- A duplicate `end` marker and an `end` with `t_ms` below the last event (the code handles it defensively via `final_t().max(events)`, but it is not pinned).
- A watermark regression: a source "speaks" for the first time after another source's jump.

## Documentation

- The `expect_nodes` caveat: with `--expect a,p`, Q verdicts do not hold back the watermark — minute windows may close before late q-events arrive (the final shift stays correct). The test `watermark_ignores_unexpected_sources` pins the intent, but spell out the consequence in the `Config::expect_nodes` docs.
- The deadline from the critical card fits naturally as a CLI flag `--max-wait-s` with a sensible default — it keeps the hang-free semantics for manual runs too.
