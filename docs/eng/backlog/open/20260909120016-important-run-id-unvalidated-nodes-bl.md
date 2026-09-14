# `run_id` is not validated

- **Severity**: important
- **Component**: nodes
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A2, Important #3
- **Location**: `nodes/src/mqtt_sink.rs:8-9` (the claim), `nodes/src/bin/node.rs:61-62` (the reality)

## Summary

A comment in `mqtt_sink.rs` claims: "escaped-free fields (run ids are validated by the CLI)" — but the CLI accepts `run_id: String` without validation. A run_id with a quote/backslash breaks the JSON in **all** payloads (`on_status`, `publish_end`).

## Suggested fix

A clap `#[arg(value_parser = ...)]` rejecting `"`, `\` and control characters (or escaping at format time — validation is simpler and more honest relative to the doc).

## Tests

run_id with special characters.
