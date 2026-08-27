# Week 1 — decomposition (Spike)

> Branch: `spike/foundation` (historically created as `spike/week1-foundation`)

> Decomposition of the "Week 1" row from [`plan.md`](../plan.md),
> section 9. The week's goal is to retire risks #1–#2 (building the fork, serializing `Conv1D`) **before**
> writing the kernel. Mode: 1 person; weekdays ~2–3 h, Saturday ~4 h; one topic per
> session (the plan section 8 rule). Week estimate: ~16–19 h.

## Week gate (minimum ready)

- [x] `predict()` works on the host: the fork's example computes; bonus — our own dense model.
      `sine` — [NOTES.md](../../../fork/NOTES.md), D1; the `dense_spike` bonus with our Keras model —
      [spike doc](../../../spike/conv1d-serialization.md), section "Bonus".
- [x] The `Conv1D` spec is written and sufficient for the implementation to proceed without guesswork.
      [fork/docs/conv1d-spec.md](../../../fork/docs/conv1d-spec.md).
- [x] The fork builds: `cargo build` + `cargo test` green (risk #1 retired or handled).
      25 lib + 3 integration tests green; [NOTES.md](../../../fork/NOTES.md), D1;
      re-checked at the gate — [week1-gate.md](../week1-gate.md).
- [x] The fact of `Conv1D` serialization into `.tflite` is documented with an operator dump (risk #2).
      [spike/conv1d-serialization.md](../../../spike/conv1d-serialization.md).
- [x] The workspace skeleton (the 5 entities from plan section 3) builds; the simulator emits the first
      deterministic CSV. 22 tests + clippy `-D warnings` green; determinism —
      the test [determinism.rs](../../../line-simulator/tests/determinism.rs) and a local
      `diff run1.csv run2.csv` (empty; details in [week1-gate.md](../week1-gate.md)).

## Day-by-day summary

| Day | Session topic  | Summary                                        | Artifact                        |
| --- | -------------- | ---------------------------------------------- | ------------------------------- |
| D0  | environment    | toolchain, venv, flatc, fork on GitHub         | tools ready                     |
| D1  | engine         | fork build + example + tests [risk #1]         | the fork builds                 |
| D2  | engine         | how `#[model]` and `predict()` work            | `NOTES.md` on the fork layout   |
| D3  | ML             | Keras model → tflite → operator dump           | `spike/conv1d-serialization.md` |
| D4  | engine         | the `Conv1D` spec — the contract for weeks 2–3 | `fork/docs/conv1d-spec.md`      |
| D5  | infrastructure | workspace skeleton + the link to the fork      | repo skeleton, `cargo test` ok  |
| D6  | simulator      | machine FSM + seeded RNG + first CSV           | `run1.csv`, diff of two runs    |
| D7  | gate           | checklist, retro, the decision for week 2      | `docs/week1-gate.md`            |

## D0 (the evening before the start, ~1 h): environment

| Tool                     | Why                                                       | Check                              |
| ------------------------ | --------------------------------------------------------- | ---------------------------------- |
| rustup + stable          | building the fork                                         | `rustc -V`                         |
| python venv + tensorflow | Keras model, tflite dump                                  | `python -c "import tensorflow..."` |
| Rust `golden-gen`        | kernel golden vectors (week 2) — same toolchain, no numpy | `cargo test` in the fork (week 2)  |
| flatc (flatbuffers)      | `.tflite` JSON dump (an interpreter option)               | `flatc --version`                  |
| git + GitHub account     | forking `microflow-rs`                                    | `git --version`                    |

QEMU is not needed this week (week 6).

## D1 (Mon, ~2 h) — engine: the fork and the build [risk #1]

1. Fork `matteocarnelos/microflow-rs` on GitHub, clone locally.
2. `cargo build --release`, then `cargo test` in the clone.
3. Run the `sine` example (or the closest one that builds) on the host.
4. Record in `NOTES.md`: the Rust version, build time, what was failing.

Check: the example prints a prediction, `cargo test` green.

Escalation: it doesn't build within an evening → risk #1 has fired, act per plan section 11 —
don't drag it out, redistribute in the morning on D2 (options in the "Escalation points" section).

## D2 (Tue, ~2–3 h) — engine: how `#[model]` and `predict()` are structured

1. Read the repo structure: the list of crates and each one's role; the list goes in `NOTES.md`.
2. Trace the model's path: `.tflite` → macro (compile-time flatbuffers parsing) →
   generated code → `predict()`.
3. Dissect the `predict()` signature: how to feed the input tensor, what is returned.
4. Compile the list of supported operators and what is missing for `Conv1D`
   (3D tensors? `Reshape`? padding?).

Check: you can describe in words the path from the model file to `predict()` — that is half
of the week's gate.

## D3 (Wed, ~2–3 h) — ML: the test model and the serialization dump [risk #2]

1. Build a Keras mini-model per the plan section 6 architecture:
   `Conv1D(8) → AvgPool → Conv1D(16) → AvgPool → FC → Softmax`, input `(T, C)`.
2. Full-integer int8 quantization, export `.tflite`.
3. Operator dump: the list of opcodes via `tf.lite.Interpreter` (or `flatc --json`).
4. Record the chain: per plan section 5, `Conv1D → Reshape → CONV_2D` over `(1, T, C)` is expected;
   the actual result may differ — the actual result is the spike's outcome.
5. Bonus: a dense model (`FC + Softmax`) → `.tflite` → substitute into the fork's example →
   `predict()` on the host. This way the full small "Keras → tflite → Rust" loop is completed at once.

Artifact: `spike/conv1d-serialization.md` — the script, the actual operator graph,
tensor shapes, the weight layout.

## D4 (Thu, ~3 h) — engine: the `Conv1D` spec

The spec is the contract for the weeks 2–3 implementation (the "spec → code → tests" rule). Sections:

1. Input from D3: the actual opcodes, tensor shapes, weight and bias layout.
2. Parser: which nodes to accept, how to fold the `Reshape` chain, the attributes
   (stride, padding), 3D tensor support.
3. Kernel: int8 dot-product, int32 accumulator, per-channel requant
   (scale/zero-point), output.
4. Codegen: which structures and buffers `#[model]` generates for the new case.
5. Tests: golden vectors from a numpy reference; edge cases — `T < kernel_size`,
   `stride = 2`, `valid/same` padding.
6. Definition of Done for week 2 (kernel) and week 3 (parser + codegen).

Check: "an outside developer implements from the spec without questions" — if not,
keep writing it up to that state.

## D5 (Fri, ~2–3 h) — infrastructure: the workspace skeleton

1. Decide the project repo's location (not in `tmp/OEE`) and the way to wire in the fork:
   git submodule vs subtree — record the choice and the reason in one line.
2. The skeleton per plan section 3: the root `Cargo.toml` (workspace), the crates
   `line-simulator`, `nodes` (A/P/Q as stub modules), `oee-aggregator`,
   `features-cli` — empty, with `lib.rs`.
3. Green from day one: `cargo test` and
   `cargo clippy --all-targets -- -D warnings`.
4. README: how to build and run.

Artifact: the workspace skeleton.

## D6 (Sat, ~3–4 h) — simulator: FSM + the first CSV

1. Seeded RNG (record the package choice) — the foundation of reproducibility (section 4).
2. The machine FSM: `idle → run → jam/overload`; the scenario is a declarative list of events,
   parsed from a file (it is also the future ground truth).
3. The current signal, first version: a 50 Hz sine + a mode envelope + seeded noise.
4. CLI: `line-simulator --scenario base.toml --seed 42 --out run1.csv`.

Check: two runs with the same seed → `diff` empty; the current plot is distinguishable by
mode at a glance.

## D7 (Sun, ~2 h) — gate and retro

1. Go through the "Week gate" checklist above; every "yes" — with a link to the artifact.
2. Update the risks (plan section 11): what came true, what didn't.
3. The decision for week 2: per plan / redistribution (if risk #1 fired).
4. The week note: actual hours vs estimate, what went wrong.

Artifact: [`docs/week1-gate.md`](../week1-gate.md) — a checklist + 5–10 lines of conclusions
(the plan listed `tmp/OEE/week1-gate.md`, but `tmp/` is gitignored — gate docs live in
`docs/`).

## Escalation points

- D1: the fork doesn't build for longer than an evening → (a) pin the Rust version from the fork's README;
  (b) search the fork's issues — a common problem is often already solved; (c) shift the workspace
  skeleton from D5 to D2, a build walkthrough with the mentor. The decision — on the day of discovery.
- D3: `Conv1D` exported without the `Reshape` chain (a native op) → the parser
  is simpler — good news, record the fact in the D4 spec.
- D3 (bonus): the dense model doesn't predict on the host → the gate is not blocked,
  the investigation moves to the beginning of week 2.

## Anti-scope (what we do NOT do in week 1)

- The `Conv1D` kernel — week 2; the parser and codegen — week 3.
- Training the real node A/Q models — weeks 3–4.
- MQTT, Node-RED, aggregator logic — weeks 4–5.
- QEMU and criterion benchmarks — week 6.
