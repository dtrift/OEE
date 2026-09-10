# The "err +0.000" claim is not pinned by a test

- **Severity**: important
- **Component**: oee-aggregator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, Important #4
- **Location**: `README.md:252-258`, `docs/eng/week5-gate.md` (the claim); `oee-aggregator/tests/experiment.rs:327-347` (the asserts)

## Summary

README and week5-gate claim an error of `+0.000` on 4 scenarios, but `experiment.rs` only asserts `|err| < 0.05` (OEE) and `< 0.08` (components), `other_seeds` — `< 0.06`. A drift up to +0.04 passes CI while README keeps promising +0.000. The table in the docs is a manual extract, not a test artifact.

## Suggested fix

A tight regression for seed 42: e.g. `|err| <= 0.001` on OEE and the components (the actually observed 0.000/−0.001 values allow it), keeping the loose bounds for sensitivities/other seeds.
