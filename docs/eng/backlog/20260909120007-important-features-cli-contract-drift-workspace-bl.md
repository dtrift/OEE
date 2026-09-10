# `features-cli` contract drift: the "single source of truth" is read by no one

- **Severity**: important (systemic, found independently by 3 reviewers)
- **Component**: workspace (features-cli + all consumers)
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), section "Systemic issues"; A2 Important #2, A5 Important #4, A6 Recommendations
- **Location**: `features-cli/src/lib.rs:48-49, 66-84`; duplicates: `nodes/src/a.rs:29` (WINDOW=128), `nodes/src/q.rs:28` (WINDOW=1024), `ml/trainer/src/lib.rs:18,22` (TIMESTEPS=128, Q_TIMESTEPS=1024), `firmware/{a,q}` (constants with a comment reference), `nodes/src/bin/node.rs:83,154` (rates 1600/16_000)

## Summary

The `features-cli` crate is declared the "single source of truth for training and firmware", but the window/rate values are duplicated by hand in every consumer. In `nodes` the dependency on `features-cli` (`nodes/Cargo.toml:14`) is dead altogether — not used by code, only by doc comments.

The values currently match, but changing `window_spec` will not break compilation — models will silently diverge from the firmware. Exactly the class of bugs the crate was created to prevent.

## Suggested fix

Minimally — contract tests in every consumer:

```rust
assert_eq!(a::WINDOW, features_cli::window_spec(NodeKind::A).unwrap().samples);
```

- in `nodes` (dependency already present);
- in `ml/trainer` (dev-dependency);
- in `firmware-a`/`firmware-q` (host tests, dependency already present);
- `qemu` can take the dependency too — features-cli is `#![no_std]`.

Better — compute the constants from `window_spec` (it is a `const fn`).

Also: either start using the dependency in `nodes`, or remove it — a dead path-dependency masks the fact that the contract is duplicated.
