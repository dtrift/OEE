# Week 2 Gate — Conv1D Kernel + Current Signal

> Formalized 2026-08-25; the week's work was done the same day per
> [decompose/step-2.md](decompose/step-2.md). When formalizing the gate, the checks were
> re-run: the fork — 33 lib + 4 integration tests, clippy clean; the workspace —
> 10 suites, clippy clean; the golden generator is idempotent (a repeated run —
> a bit-for-bit identical fixture file).

## Gate checklist

| Gate item                                                         | Status | Artifact / check                                                                                                                            |
| ----------------------------------------------------------------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `conv_1d`: toy §5.1 + golden §5.2 bit-for-bit                     | yes    | [conv_1d.rs](../../fork/microflow/src/ops/conv_1d.rs) — 8 tests; [golden](../../fork/microflow/tests/conv1d_golden.rs) — 96 cases (seed 42) |
| Edge cases + `no_std`                                             | yes    | `T<k` (same), `stride=2`, valid/same, 8 channels; beyond: T=1/63/64, saturation, ties-even, zp_x-neutral padding                            |
| `cargo test` + clippy green (fork + workspace)                    | yes    | fork: 33+4; workspace: 10 suites; clippy 0 (fork — with the CI flag `-A mismatched_lifetime_syntaxes`)                                      |
| Simulator: parameters in the scenario, separability "not perfect" | yes    | `[signal]`/`[noise]` sections; RMS tests ([signal.rs](../../line-simulator/src/signal.rs)); base/downtime/degradation scenarios             |

## Risks (plan section 11)

- Risk #2 (the parser is harder than expected) — was not touched this week (the parser is
  week 3); spec §2 is ready, the actual scope has been known since the spike.
- "Too-good models" — handled preventively: amplitude drift (`drift_sigma`) and noise are
  scenario parameters; the test fixes "distinguishable, but not perfect" (window mean RMS values
  differ, there is spread within a mode).
- The week's escalations (kernel/reference divergence, the kernel dragging on) did not
  materialize: bit-for-bit agreement was reached, the D4 buffer was not needed.

## Deviations from the decomposition

- `golden-gen` — an example, not `src/bin`: dev-dependencies are unavailable to a bin
  (the only deviation from the D3 plan, the semantics are the same).
- §3.3 (FC per-channel + optional bias) — was not needed for the gate; the D4 plan
  allowed moving it — it shifts to week 3.
- Requant — an f32 multiplier inside the kernel (spec §3.1), not macro preprocessing
  constants as in `conv_2d`: week 3 codegen only needs to pass scale/zp and the int32 bias
  through. Recorded in [fork/NOTES.md](../../fork/NOTES.md).

## Decision for week 3

Per the plan, no reshuffling required: the parser (shape-folding §2.1–2.3, rank-3 input) +
codegen (§4, kernel call) + ML pipeline (model A, dataset, parity). The inputs are ready:
spec §2/§4, a kernel with an API for codegen, a parameterized signal, and dataset scenarios.
Details — [decompose/step-3.md](decompose/step-3.md).

## Retro

- Hours were not tracked: the agreement from [week1-gate.md](week1-gate.md) about time notes
  at the end of sessions did not work — there is no comparison with the ~16–19 h estimate.
- Two arithmetic errors in manual unit-test expectations (the m(f) multiplier, the
  multi-channel case) were caught by runs: compute manual numbers from the spec's formulas
  and double-check them; the golden generator closes this class of errors entirely.
- The `feat/conv1d-kernel` branch in the fork is created before the commit; the OEE branch
  exists.
