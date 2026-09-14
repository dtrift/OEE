# Minor: line-simulator

- **Severity**: minor
- **Component**: line-simulator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A1, "Minor"

A consolidated list of minor remarks (each item: file:line, the point, the action):

1. `scenario.rs:12` — `DEFAULT_DURATION_MS` is used nowhere in the workspace while `duration_ms` is a required field. Dead and misleading code: remove or make a real `#[serde(default = ...)]`.
2. `lib.rs:53-56` — `Simulator::state()` is called by no one in the project. Dead pub API.
3. `signal.rs:73-76` — `window_rms` is used only by its own module's tests; move it under `#[cfg(test)]`/to a consumer, or mark it as a utility.
4. Events with `t_ms >= duration_ms` are silently never applied (the main loop `main.rs:67-75` ends earlier). Reject them in `Scenario::parse` with a message — a scenario typo is invisible now.
5. `main.rs:60` — the parse error loses the file path: `.with_context(|| format!("parsing {}", args.scenario.display()))`.
6. `--dataset -`, `--taps-dataset -`, `--belt-events -` are not supported although `--out -` exists (`main.rs:123`) — a UX asymmetry.
7. Two CSV-writing mechanics: `write_raw` via `csv::Writer`, the rest via manual string assembly; `dataset.rs:49-72` and `taps.rs:139-158` are nearly identical — a shared helper `write_labeled_csv(windows, n_cols, writer)` is due.
8. `main.rs:145` — `window_len = 128` as a literal against the constant `taps::TAP_WINDOW` (`taps.rs:34`). Introduce `pub const CURRENT_WINDOW: usize = 128;` next to `SAMPLE_RATE_HZ`.
9. `dataset.rs:25-28` — `assert!` in a library function; for a CLI, `value_parser = clap::value_parser!(usize).range(1..)` is more correct — a panic becomes impossible in principle.
10. `lib.rs:6` — the doc "CSV output \"time, current, true mode\"" does not match the actual header `t_ms,current_a,state`.
11. Part arrival times in taps are strictly multiples of `period_ms` without jitter (`taps.rs:86, 105`), unlike belt — a detector could theoretically "cheat" by time. Record this as a deliberate decision in the module doc.
12. `Scenario::parse` returns `Result<_, String>` (`scenario.rs:266`) — stringly-typed errors; a `thiserror` enum or `Box<dyn Error>` would give call sites more context.
