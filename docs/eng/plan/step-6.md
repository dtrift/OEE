# Week 6 — decomposition (QEMU + benchmarks + report + demo)

> Branch: `feat/qemu-report` (OEE) + `bench/conv1d-vs-conv2d` (fork/microflow)

> Decomposition of the "Week 6" row from [`plan.md`](../plan.md),
> section 9. The final week: portability (QEMU LM3S6965), numbers (criterion,
> flash/RAM), the report and a recorded demo. There is no reserve after it — what's unfinished gets cut
> per the priority list (escalations). Mode: 1 person; weekdays ~2–3 h, Saturday
> ~4 h. Estimate: ~16–19 h. Entry: the full pipeline on the host, the measured vs truth table.

## Week gate (minimum ready)

- [ ] QEMU: the fork's example and our own model on LM3S6965, output via UART.
- [ ] Footprint: flash/RAM from the ELF; Δ "with Conv1D vs without" and vs the reshape trick.
- [ ] criterion benchmarks: min/avg/max latency, Conv1D vs the Conv2D trick.
- [ ] The report is assembled (results, limitations); the demo is recorded and reproduces.

## Day-by-day summary

| Day | Session topic | Summary                                        | Artifact                   |
| --- | ------------- | ---------------------------------------------- | -------------------------- |
| D1  | QEMU          | bench: qemu lm3s6965evb, toolchain, examples   | the example runs over UART |
| D2  | QEMU          | our own no_std binary with model A             | predict over UART          |
| D3  | footprint     | flash/RAM from the ELF, comparisons            | the footprint table        |
| D4  | benchmarks    | criterion: Conv1D vs the trick, latencies      | a table of numbers         |
| D5  | report        | results, limitations, conclusions              | the full report draft      |
| D6  | demo          | the recording scenario, a one-command launch   | a demo video               |
| D7  | final         | reproducibility from a clean clone, submission | everything assembled       |

## D1 (Mon, ~2–3 h) — QEMU: the bench

1. `qemu-system-arm -M lm3s6965evb`; the `thumbv7m-none-eabi` toolchain (section 7).
2. Run the fork's examples on QEMU, output via UART.
3. Record the QEMU/toolchain versions and the commands — into the README/report.

Check: the fork's example prints to the UART console.

## D2 (Tue, ~3 h) — QEMU: our own model

1. A minimal no_std binary: model A (or the P logic) + UART output.
2. Run on QEMU; cross-check the predictions against the host — they must match.
3. Being honest: the timings are not cycle-accurate (the section 11 risk) — into the report's limitations.

Check: QEMU and the host give identical predictions on the same inputs.

## D3 (Wed, ~2 h) — footprint

1. `cargo-size` / from the ELF: the flash/RAM of the binary with the model.
2. Comparisons: a dense baseline without Conv1D vs with Conv1D; vs the Conv2D reshape trick.
3. The footprint table — into the report's "results" section.

## D4 (Thu, ~3 h) — criterion benchmarks

1. Criterion on the host (the main source of numbers, section 7): the honest Conv1D
   kernel vs Conv2D-through-reshape; the min/avg/max latency of the A/Q models.
2. Several runs; fix the numbers in a table.
3. Into the report: the raw results + 3–5 interpretation takeaways.

## D5 (Fri, ~3–4 h) — the report

1. Structure: the task, architecture, the engine contribution, experiments (the week 5
   OEE table), engine/node metrics (confusion matrices), limitations, future work.
2. The sections from plan section 10: OEE correctness, the engine, the nodes, determinism.
3. Read it through once end-to-end with your own eyes: the logic of the exposition, not just the facts.

## D6 (Sat, ~3–4 h) — the demo

1. The recording scenario (2–4 min): the bench in operation, the dashboard, a micro-stop
   and a fail changing OEE before your eyes.
2. Launch everything with one command: the week 5 script (mosquitto + simulator + nodes +
   `cargo run -p oee-dashboard`); the terminal full-screen — a clean recording.
3. The recording + a short backup take.

## D7 (Sun, ~2 h) — the final gate

1. Reproducibility: a clean clone → build → run → the numbers matched.
2. The week's checklist; the loose ends — into the report's future work, not into "I'll finish it soon".
3. Submission: everything assembled, the demo reproduces — the final line of the plan.

Artifact: `tmp/OEE/week6-gate.md` + the report and the video.

## Escalation points

- The QEMU part doesn't go → priority: the benchmarks and the report are more important;
  describe QEMU honestly as a partial result (the fork's examples run, our own model doesn't, and why).
- Not enough time for everything → cut in this order: QEMU details → dashboard eye candy →
  benchmark exotica. Do NOT cut: the OEE table, the engine metrics, the demo.
- The report risks becoming a "chronicle" → the format: one conclusion = one table/chart +
  2–3 sentences; the chronicle — into an appendix.

## Anti-scope (what we do NOT do in week 6)

- An upstream PR — after the course (section 12); the camera and QAT — future work.
- New experiments — only if a gap is visible in the report.
- Simulator improvements "for beauty" — a feature freeze, bugfixes only.
