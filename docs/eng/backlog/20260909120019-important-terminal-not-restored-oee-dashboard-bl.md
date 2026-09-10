# Terminal is not restored on the error path

- **Severity**: important
- **Component**: oee-dashboard
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, Important #2
- **Location**: `oee-dashboard/src/main.rs:72-79`

## Summary

`event::poll(TICK)?`, `event::read()?`, `terminal.draw(...)?` — any io error exits `main` via `?` **bypassing** `ratatui::restore()`. Panics are covered by the hook, errors are not: the user gets a broken terminal (raw mode + alternate screen), and the error message goes to the "broken" stdout.

## Suggested fix

Extract the loop into a function and guarantee `ratatui::restore()` on every exit (a drop guard or a `match` around the loop's result).
