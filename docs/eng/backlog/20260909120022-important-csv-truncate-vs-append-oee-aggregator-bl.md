# CSV: documentation promises append, code truncates

- **Severity**: important
- **Component**: oee-aggregator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, Important #5
- **Location**: `oee-aggregator/src/bin/aggregator.rs:7` (doc "appends the windows CSV"), `aggregator.rs:307` (`WindowsCsv::create` → `File::create`)

## Summary

Re-running with the same `--out` erases the previous run, although the documentation promises appending.

## Suggested fix

Either:

- `OpenOptions::new().append(true)` with a header only for a new file;
- or an honest "writes (truncates)" wording in the doc and a unique `--out` name in the bench script.
