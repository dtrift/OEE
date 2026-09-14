# `clamp(1, 0)` panic in `split` when a class is missing

- **Severity**: critical
- **Component**: ml-trainer
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A5, Critical #1
- **Location**: `ml/trainer/src/data.rs:100-105`

## Summary

`cut.clamp(1, idx.len())`: if a class has zero windows, `clamp(1, 0)` panics ("min > max"). The trigger is quite realistic:

- `--task q` on a taps dataset without rejects (only "good") during week-4 bring-up;
- passing an incomplete set of CSVs for A.

`load_csv` validates the label range but not the presence of all classes; `split` returns a tuple instead of a `Result` — the pipeline crashes with an obscure diagnostic after successfully loading the CSVs.

## Suggested fix

- In `load_datasets`, check `counts[c] > 0` for all `0..num_classes` and return `Err("class 'cracked' has no windows in the given datasets")`.
- In `split`, add `if idx.is_empty() { continue; }` as a safety net.
