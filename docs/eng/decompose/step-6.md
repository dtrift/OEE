# Week 6 — decomposition (QEMU + benchmarks + report + demo)

> Branch: `feat/qemu-report` (OEE) + `bench/conv1d-vs-conv2d` (fork/microflow)

> Breakdown of the "Week 6" row of the plan [`plan.md`](../plan.md), plan section 9.
> The final week: portability (QEMU LM3S6965), numbers (criterion, flash/RAM), the
> report and a recorded demo. There is no reserve after it — anything unfinished gets
> cut per the priority list (escalations). Mode: 1 person; weekdays ~2–3 h, Saturday
> ~4 h. Estimate: ~16–19 h.
>
> Input: the week 5 gate — the full pipeline on the host, the measured vs truth table,
> a one-command bench launch. Precedents in the fork: `examples/qemu/` (its own
> `Cargo.toml`, `memory.x`, `Makefile.toml` — a template for our own QEMU binary),
> `benches/{sine,speech,person_detect}.rs` — a criterion bench template.
>
> Detailing of the draft [`plan/step-6.md`](../plan/step-6.md): tied to the fork's
> precedents and the week 5 artifacts; on conflict → this file is edited, the plan only
> in substance.

## Week gate (minimum done)

- [ ] QEMU: the fork's example and our own model A on LM3S6965, output via UART;
      predictions match the host.
- [ ] Footprint: flash/RAM from the ELF; Δ "with Conv1D vs without" and vs the reshape
      trick.
- [ ] Criterion: model min/avg/max latency; Conv1D vs the Conv2D trick.
- [ ] The report is assembled (results, limitations); the demo is recorded and
      reproduces from a clean clone.

## Day-by-day summary

| Day | Session topic | Essence                                        | Artifact                     |
| --- | ------------- | ---------------------------------------------- | ---------------------------- |
| D0  | infra         | qemu-system-arm, toolchain, branches           | the fork binary under target |
| D1  | QEMU          | running the fork's examples/qemu over UART     | the example prints to UART   |
| D2  | QEMU          | our own no_std binary with model A             | predict in UART = host       |
| D3  | footprint     | flash/RAM from the ELF, comparisons            | footprint table              |
| D4  | benchmarks    | criterion: Conv1D vs the trick, latencies      | a table of numbers           |
| D5  | report        | results, limitations, conclusions              | the full report draft        |
| D6  | demo          | recording scenario, one-command launch         | a demo video recording       |
| D7  | final         | reproducibility from a clean clone, submission | everything assembled         |

## D0 (evening before start, ~0.5–1 h): environment

1. Install `qemu-system-arm`; `rustup target add thumbv7m-none-eabi` (plan section 7).
2. Check: the fork's example builds for the target (`examples/qemu`, cargo build).
3. Branches: `bench/conv1d-vs-conv2d` in the fork, `feat/qemu-report` in OEE.

## D1 (Mon, ~2–3 h) — QEMU: the bench

1. Running the fork's `examples/qemu`: `qemu-system-arm -M lm3s6965evb` + UART output
   (the example's `Makefile.toml` is the template).
2. Pin the QEMU/toolchain versions and the commands — into the README and the report
   (D7 reproducibility).

Check: the fork's example prints to the UART console.

## D2 (Tue, ~3 h) — QEMU: our own model

1. A minimal no_std binary modeled on `examples/qemu`: model A + UART output of
   probabilities/argmax on fixed windows.
2. A run on QEMU; a cross-check against the host — bit-for-bit: the kernel's integer
   semantics are deterministic (spec §3, weeks 2–3).
3. Honestly into the report's limitations: QEMU timings are not cycle-accurate (the
   plan section 11 risk); the primary speed numbers — criterion on the host.

Check: QEMU and the host give identical predictions on the same inputs.

## D3 (Wed, ~2 h) — footprint

1. `cargo-size`/ELF analysis: flash/RAM of the binary with model A.
2. Comparisons: the dense baseline (without Conv1D) vs with Conv1D vs the reshape trick
   (the same model through the Conv2D path).
3. The footprint table — into the report's "results" section.

## D4 (Thu, ~3 h) — criterion benchmarks

1. A new `benches/conv1d.rs` modeled on `benches/sine.rs`: a fair `conv_1d` vs
   `conv_2d` with h=1 (the reshape trick); the `predict()` latency of the A/Q models
   per window.
2. Several runs; pin the numbers in a table (plan section 10: "Conv1D vs the reshape
   trick").
3. Into the report: raw results + 3–5 interpretation conclusions.

## D5 (Fri, ~3–4 h) — report

1. Structure: the task, architecture, the engine contribution (spec → kernel →
   parser, golden/parity), experiments (the week 5 OEE table), metrics (A/Q confusion,
   footprint, benchmarks), determinism, limitations, future work (plan sections 10, 12).
2. Sections from plan section 10: OEE correctness, the engine, the nodes, determinism.
3. Read it through once end to end: the logic of the narrative, not just the facts.

## D6 (Sat, ~3–4 h) — demo

1. The recording scenario (2–4 min): the bench running, the dashboard; a micro-stop
   and a reject change OEE before your eyes.
2. One-command launch — the week 5 script (mosquitto + simulator + nodes + `cargo run
   -p oee-dashboard`); a full-screen terminal — a clean recording.
3. The recording + a short backup take.

## D7 (Sun, ~2 h) — the final gate

1. Reproducibility: clean clone → build → run → the numbers matched.
2. The week's checklist; unfinished items — into the report's future work, not into
   "will finish soon".
3. Submission: everything assembled, the demo reproduces — the final row of the plan.

Artifact: `docs/week6-gate.md` + the report and the video (in the early plan —
`tmp/OEE/week6-gate.md`; `tmp/` is gitignored, precedent — week 1).

## Escalation points

- The QEMU part will not go → priority: benchmarks and the report matter more;
  describe QEMU honestly as a partial result (the fork's examples run, our own model
  does not, and why).
- Not enough time for everything → cut in order: QEMU details → dashboard polish →
  benchmark exotica. Do NOT cut: the OEE table, engine metrics, the demo.
- The report threatens to become a "chronicle" → format: one conclusion = one
  table/graph + 2–3 sentences; the chronicle — into an appendix.
- The numbers jump between criterion runs → pin the settings (iters, sample-size),
  run on a quiet machine; the variability — into the limitations.

## Anti-scope (what we do NOT do in week 6)

- An upstream PR — after the course (plan section 12); camera and QAT — future work.
- New experiments — only if a hole is visible in the report.
- Simulator improvements "for beauty" — a feature freeze, bugfixes only.
