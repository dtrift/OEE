# `--filter` does not work: the `oee/line1/` prefix is hardcoded

- **Severity**: important
- **Component**: oee-dashboard
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, Important #3
- **Location**: `oee-dashboard/src/state.rs:99` (strip_prefix), `main.rs:35-36` (CLI `--filter`)

## Summary

`topic.strip_prefix("oee/line1/")` is hardcoded while the CLI offers `--filter`. With `--filter oee/line2/#` the subscription works, but every message is stripped away: the UI is silently empty, no errors.

## Suggested fix

- Derive the prefix from `--filter` (strip the `/#` or `...` tail) and pass it into `DashboardState`;
- or remove `--filter` from the CLI.
