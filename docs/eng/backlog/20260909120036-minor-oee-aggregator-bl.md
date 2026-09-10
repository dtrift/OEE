# Minor: oee-aggregator

- **Severity**: minor
- **Component**: oee-aggregator
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A3, "Minor" (#6-8, 11-15)

A consolidated list of minor remarks:

1. `experiment.rs:143-146` — dead code: the `artifact` closure is immediately silenced by `let _ = artifact;`. Remove.
2. `aggregator.rs:167, 175, 183, 190` — `SOURCES.iter().position(...).unwrap()` in the hot path. The `a/p/q` indices are static: `const AT_A: usize = 0;` or `fn at(node) -> Option<usize>` without `unwrap`.
3. `payload.rs:78-85` — `find_key` allocates a `String` per lookup, and there are several lookups per message (`run_id`, `t_ms`, the payload field). Search for `"` + key + `":` char-by-char or via `memchr`.
4. `bin/aggregator.rs:50-52` — `--expect` silently drops unknown names: `--expect a,zz` becomes `a` without a warning (a typo = waiting for the wrong markers). Fail on an unknown name. Likewise `Aggregation::new` (`aggregator.rs:128`) does not validate `expect_nodes` at all — a typo in a lib call = a silent hang.
5. `aggregator.rs:379-387` — `ready` is signaled before the CSV is created: if `WindowsCsv::create` fails, the nodes are already publishing into the void. Move the CSV creation above `ready.send`.
6. `windows.rs:73` — `stretch_from.max(from)` is redundant: `stretch_from` is always `>= from` at that point. Dead guard.
7. `aggregator.rs:441-443` — O(n) scans per message: `shift_snapshot` is published after *every* message and re-scans all `statuses/counts/verdicts` from scratch → O(n²) per run. Invisible for the bench; when scaling — `partition_point`/cursors.
8. The fixed client id `"oee-aggregator"` (`aggregator.rs:373`): two instances on one broker — mosquitto will drop the older connection. Fine for the bench; note it in the docs (see also `20260909120037-minor-oee-dashboard-bl.md`).
