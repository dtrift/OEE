# Minor: firmware

- **Severity**: minor
- **Component**: firmware
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, "Minor"

A consolidated list of minor remarks:

1. DRY: `Cursor` is copied 3 times (`firmware/a/src/lib.rs:189-216`, `q/src/lib.rs:79-105`, `p/src/lib.rs:65-92`). Extract into a shared crate (a module in `board` or a separate `fmt-util`).
2. DRY: the argmax snippet is repeated 4 times (`qemu/src/lib.rs:44-50`, `qemu/src/bin/dense.rs:39-44`, `firmware/a/src/lib.rs:47-53`, `firmware/q/src/lib.rs:38-44`). At least a shared helper in `firmware-a`/`firmware-q`.
3. `firmware/a/src/lib.rs:175-185` — `format_capture` is not used by target code (tests only). Mark it as the capture mode in NOTES/README or remove it until a consumer appears.
4. `firmware/q/src/lib.rs:131-146` — the test `classify_matches_the_host_node_q` is effectively vacuous (`verdict < 2` is always true), honestly admitted in a comment; but `ml/models/model_q.val.csv` exists — pin a real window like `firmware-a` does.
