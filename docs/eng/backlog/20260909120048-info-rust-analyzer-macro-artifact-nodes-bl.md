# Diagnostics at `nodes/src/a.rs:24`: 10 errors — a rust-analyzer artifact

- **Severity**: info (needs verification, not a fix)
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), section "Systemic issues"; A2, "Diagnostics a.rs:24"
- **Location**: `nodes/src/a.rs:24`; the root — `fork/microflow/microflow-macros/src/lib.rs:77, 91, 346`

## Summary

rust-analyzer shows 10 errors ("expected [i8; 1], found [{unknown}; 0]", "expected [f32; 4], found [f32; 3]") at the `#[model(...)]` attribute line. The reviewer's conclusion: this is not a code error but a stale cached proc-macro expansion:

- all proc-macro spans collapse to the call site — any type error inside the generated code is reported at the attribute line;
- the macro reads the `.tflite` by a relative path and the env (`MICROFLOW_CONV2D_ONLY`) at expansion time — cargo does not fingerprint either (documented in the fork itself, `lib.rs:70-75`);
- RA has its own macro-dylib cache: the expansion may come from an older model (`[f32; 3]` looks like the trace of the old 3-class model — the doc at `a.rs:5-8` says the model was recently switched);
- the tests that actually run `CurrentModel::predict` pass in CI under `clippy -D warnings` — a real compile error would fail the same way.

## Actions

1. `cargo check -p nodes` from the workspace root — expected clean.
2. Restart rust-analyzer ("rust-analyzer: Restart Server").
3. In the fork (separately): resolve the model path from `CARGO_MANIFEST_DIR` instead of the cwd.

If `cargo check` reproduces the errors — escalate this card.
