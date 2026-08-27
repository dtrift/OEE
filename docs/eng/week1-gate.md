# Week 1 Gate — Decomposition (Spike)

> Formalized 2026-08-25. Week 1 work was completed 2026-08-20 (commit `b3d98bb`, PR #1 from
> `spike/foundation`). The checklist is from [step-1.md](plan/step-1.md). When formalizing the
> gate, the checks were re-run: `cargo test` + `cargo clippy --all-targets -- -D warnings` in
> the workspace and in the fork — green; `diff run1.csv run2.csv` (seed 42) — empty.

## Gate checklist

| Gate item                                                            | Status | Artifact / check                                                                                |
| -------------------------------------------------------------------- | ------ | ----------------------------------------------------------------------------------------------- |
| `predict()` on the host (`sine`); bonus — dense model                | yes    | [NOTES.md](../../fork/NOTES.md) (D1); [spike doc](../../spike/conv1d-serialization.md), "Bonus" |
| The `Conv1D` spec is sufficient for implementation without guesswork | yes    | [conv1d-spec.md](../../fork/docs/conv1d-spec.md)                                                |
| The fork builds, `cargo test` green                                  | yes    | 25 lib + 3 integration; NOTES.md, D1; re-verified 2026-08-25                                    |
| `Conv1D` serialization documented with an ops dump                   | yes    | [conv1d-serialization.md](../../spike/conv1d-serialization.md)                                  |
| Workspace skeleton builds; deterministic CSV                         | yes    | 22 tests + clippy; [determinism.rs](../../line-simulator/tests/determinism.rs)                  |

## Risks (plan section 11)

- **#1 "Building MicroFlow on the host" — did not materialize.** The fork built within an evening
  (~10 s release, tests green). A nuance for week 3: the `nalgebra` git-patch from the fork's
  manifest does not apply through a path dependency — a `[patch.crates-io]` section is already
  prepared in the root `Cargo.toml` (commented out until the dependency appears).
- **#2 "The TFLite parser is harder than expected" — materialized.** Fact instead of
  `Reshape → CONV_2D`: `EXPAND_DIMS → CONV_2D → RESHAPE`, dynamic Flatten
  (`SHAPE/STRIDED_SLICE/PACK`), per-channel FC weights and optional bias. All covered by the spec
  (§2.1, §2.3, §3.3): week 2 (kernel) does not change, week 3 (parser) is bulkier than "understand
  the Reshape chain", but bounded by the spec.
- The other risks did not concern week 1 (models, QEMU, timings — weeks 4–6).

## Decision for week 2

Per the plan, no reshuffling required: the `conv_1d` kernel per spec §3, DoD — spec §6
(toy test + golden cases, edge cases, clippy). Decomposition —
[step-2.md](plan/step-2.md).

## Retro

- The week estimate was ~16–19 h; actual hours by day were not logged — there is no exact
  comparison. From week 2 — a time note at the end of each session.
- The spike did its job: both targeted risks were removed/handled before writing the kernel.
- Beyond the plan (in parallel, outside the week's scope): the hardware track — the `features-cli`
  contracts, `SensorSource` in `nodes`, the `firmware/` skeleton (PR #2); CI for both workspaces.
- Small things for the future: TF requires Python 3.12 (venv in `tmp/`); the converter drops the
  zero FC bias; `target/microflow-expansion.rs` — quick codegen debugging.
