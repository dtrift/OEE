# Week 5 Gate — Node P, the OEE Aggregator, the Dashboard, the Experiment

> Implemented 2026-09-03 per [decompose/step-5.md](decompose/step-5.md) on
> branch `feat/aggregator-dashboard` (OEE only; the fork is untouched). All
> checks re-run at formalization time: the whole workspace — 17 suites,
> 168 tests green; clippy clean (the fork's pre-existing generated-code
> warnings stay silenced by the CI flag); `cargo test -p oee-aggregator
> --test experiment` is the experiment entry point.

## Gate checklist

| Gate item                                                          | Status | Artifact / check                                                                                                                                                                             |
| ------------------------------------------------------------------ | ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Node P counts parts: jitter, doubles, skips handled; count = truth | yes    | `nodes/tests/node_pipeline.rs` — `node_p_count_matches_the_belt_truth` (131 parts, 9 doubles merged, skips not counted); the P-threshold sweep below shows the design window                 |
| The aggregator publishes `oee/line1/oee` + components over windows | yes    | `oee-aggregator` (`aggregator.rs`, minute + shift scopes); `tests/aggregator_pipeline.rs` — subscribe → fold → publish → flush on `{node}/end`, consumed by a real subscribed client         |
| TUI dashboard (ratatui): live OEE, A, P, Q, counter, Q verdicts    | yes    | `oee-dashboard` — gauges with plan §1 zones (green ≥ 85%, yellow ≥ 60%), part counter, machine status, verdict ticker, minute sparkline; corrupt payload → counted, never a panic; `q` exits |
| The "scenario → true OEE → measured → error" table, 4 scenarios    | yes    | `tests/experiment.rs` — the table below; a repeat with one seed → identical result (`same_seed_produces_an_identical_result`: identical final row + identical windows CSV)                   |
| Other seeds: bounded errors, conclusions do not flip               | yes    | seeds 7 and 2026: err +0.000 across components (the in-distribution pipeline is exact); sensitivity tables below show where error does appear                                                |

## The main table (measured vs true, seed 42)

The full component breakdown (regenerate:
`cargo test -p oee-aggregator --test experiment -- --nocapture`):

| scenario | seed | true OEE | measured | err    | true A | meas A | err    | true P | meas P | err    | true Q | meas Q | err    | parts |
| -------- | ---- | -------- | -------- | ------ | ------ | ------ | ------ | ------ | ------ | ------ | ------ | ------ | ------ | ----- |
| normal   | 42   | 0.841    | 0.841    | +0.000 | 0.913  | 0.913  | +0.000 | 0.956  | 0.956  | +0.000 | 0.964  | 0.964  | +0.000 | 131   |
| downtime | 42   | 0.516    | 0.516    | +0.000 | 0.610  | 0.609  | -0.001 | 0.973  | 0.974  | +0.001 | 0.870  | 0.870  | +0.000 | 89    |
| slowdown | 42   | 0.612    | 0.612    | +0.000 | 0.920  | 0.920  | +0.000 | 0.746  | 0.746  | +0.000 | 0.891  | 0.891  | +0.000 | 103   |
| rejects  | 42   | 0.478    | 0.478    | +0.000 | 0.913  | 0.913  | +0.000 | 0.956  | 0.956  | +0.000 | 0.547  | 0.547  | +0.000 | 131   |

Each scenario hits its target component (A 0.61 on downtime, P 0.75 on the
520 ms slowdown vs the 400 ms nominal ideal, Q 0.55 at p(crack)=0.5) — the
table is not flat, and the measured side tracks it exactly.

**Why zero error, honestly.** The zero is the construction working, not a
cooking of numbers, and each term is explainable:

- **P** is exact by design (the D1 gate): doubles merge into one part,
  skips produce nothing, and the P error then only inherits the run-time
  error through the denominator.
- **A**: the hysteresis delays every transition by the same ~160 ms, so the
  run-time stretches keep their length (start and end shift equally); the
  measured planned time is 59 999 ms vs 60 000 (one sample step) — a
  rounding-level 2e-5 effect.
- **Q**: the model is in-distribution here (training noise 0.10–0.15,
  these scenarios 0.12–0.13) and classifies all taps correctly.

The honest question "when does it break?" is what the sensitivity tables
answer — the D5 deliverable.

## Sensitivity (D5): where the error actually lives

**Simulator noise (the current-signal channel)** — the full bench, the
`normal` shape + a 150 ms jam blip, noise swept 0.12 → 0.60:

| sigma_a | true A | meas A | err A  | true OEE | meas OEE | err OEE |
| ------- | ------ | ------ | ------ | -------- | -------- | ------- |
| 0.12    | 0.911  | 0.911  | -0.000 | 0.618    | 0.618    | +0.000  |
| 0.25    | 0.911  | 0.911  | -0.000 | 0.618    | 0.618    | +0.000  |
| 0.40    | 0.911  | 0.911  | -0.000 | 0.618    | 0.618    | +0.000  |
| 0.60    | 0.911  | 0.911  | -0.000 | 0.618    | 0.618    | +0.000  |

Observation 1: **gaussian amplitude noise does not move node A** up to
5× the training level — the classes (0.4/2.0/3.2/4.5 A envelopes) are too
far apart for window-level RMS to confuse. The A limit is *temporal*, not
amplitude.

**A hysteresis depth** (`confirm_after` windows; the scenario carries a
150 ms jam blip, around the node's resolution):

| confirm_after | true run ms | meas run ms | err ms | note         |
| ------------- | ----------- | ----------- | ------ | ------------ |
| 1             | 54650       | 54640       | -10    |              |
| 2             | 54650       | 54640       | -10    | line default |
| 3             | 54650       | 54800       | +150   |              |
| 4             | 54650       | 54800       | +150   |              |

Observation 2: the blip (150 ms < 2 windows + confirm) is invisible at
depth ≥ 3 — the run-time overcounts by exactly the blip. The measurement's
temporal resolution is `confirm_after × 80 ms`; events shorter than that
are lost. That is the honest A error channel of this architecture.

**P anti-double window** (over the same belt stream: 130 parts, 9 doubles;
doubles' second rise at 70 ms, real parts ≥ ~280 ms apart):

| window ms | parts | truth | merged | note                         |
| --------- | ----- | ----- | ------ | ---------------------------- |
| 50        | 139   | 130   | 0      | too narrow: doubles re-count |
| 80        | 130   | 130   | 9      |                              |
| 100       | 130   | 130   | 9      | line default                 |
| 200       | 130   | 130   | 9      |                              |
| 300       | 127   | 130   | 12     | too wide: real parts merge   |

Observation 3: P is exact anywhere in [70 ms, ~280 ms] — a 3-4× margin
around the default 100 ms. Below the double-pulse span it overcounts by
exactly the doubles (139 = 130 + 9); above the shortest real interval
(period × (1-2·jitter)) it starts merging real parts.

**Q tap noise** (node Q directly; p(crack)=0.25, `crack_noise_boost` 4×):

| noise_sigma | taps | accuracy | true Q | meas Q |
| ----------- | ---- | -------- | ------ | ------ |
| 0.01        | 145  | 1.000    | 0.717  | 0.717  |
| 0.04        | 145  | 1.000    | 0.717  | 0.717  |
| 0.08        | 145  | 0.793    | 0.717  | 0.924  |
| 0.12        | 145  | 0.717    | 0.717  | 1.000  |

Observation 4: Q degrades asymmetrically under distribution shift — noise
first eats the *cracked* recall (the dull ring drowns in rattle), so
measured Q over-reports quality (0.92, then 1.00 while the truth is 0.72).
A confirmation-threshold/PR study is the week-6/report item (plan §10).

## What was built (per day of the decomposition)

- **D0** — branch `feat/aggregator-dashboard`; the week-4 cut-line read
  (Q stays); a fresh week-4 bench run as the input.
- **D1** — the belt channel: `line-simulator/src/belt.rs` + the `[belt]`
  scenario section (validated: pulse pairs must fit inside one nominal
  slot), `--belt-events/--belt-meta` CLI, deterministic (salted seed — an
  independent stream from taps). Node P: `nodes/src/p.rs`
  (`EdgeCounter`: rising-edge detect + the 100 ms anti-double merge),
  `IrSource` over the events CSV (bad rows isolate), publishing
  `oee/line1/p/count` `{count,t_ms,run_id}`; `node --kind p`.
- **D2** — `mqtt-min` learned SUBSCRIBE/SUBACK (QoS 0), incoming-PUBLISH
  parsing with idle-aware reads (a timeout never desyncs mid-packet), and
  the loopback broker became a real one: per-connection reader+writer
  threads, wildcard (`#`/`+`) dispatch, subscription cleanup — plus the
  `broker` binary for the offline bench. The aggregator: event-time
  windows driven by a **watermark** (`min` of the sources' last seen
  machine time; per-source order is guaranteed by one TCP connection per
  node) — deterministic despite thread interleavings; minute windows + a
  cumulative shift view; `oee/line1/oee` payloads (the pinned D3 contract,
  hand-formatted JSON both ways); the windows CSV; flush on
  `oee/line1/{node}/end` markers (each node publishes one after its
  stream); the `aggregator` CLI.
- **D3** — `oee-dashboard` (the 6th crate, whole stack in Rust): an MQTT
  thread (subscribe `oee/line1/#`, reconnect with backoff, liveness ticks)
  → mpsc → a pure `DashboardState::on_message` (unit-tested; corrupt
  payloads counted, never fatal) → `ui(frame, &state)` (ratatui Gauges
  with the plan §1 zones, counter, status, verdict ticker, Sparkline;
  rendered against a TestBackend in tests; values clamped before Gauge —
  hostile input cannot panic the render). `q` exits;
  `ratatui::init()`'s panic hook restores the terminal.
- **D4** — the experiment (`tests/experiment.rs`): the four scenarios of
  `scenarios/week5/` (normal/downtime/slowdown/rejects), each a full
  in-process bench (loopback broker + 3 node threads + aggregator thread),
  truth computed from the scenario alone (run intervals, belt meta, tap
  histogram); artifacts in `tmp/experiment/`.
- **D5** — the sensitivity tables above (noise / hysteresis / anti-double
  window / tap noise); `run_a_confirmed` parameterized the hysteresis for
  the sweep.
- **D6** — determinism (one seed → an identical final row and an identical
  windows CSV, `run_id` aside) and other seeds (7, 2026); the one-command
  bench `scripts/bench.sh` (broker + simulator + nodes + aggregator +
  dashboard); README updated (both languages).
- **D7** — this gate (rus/eng pair).

## Deviations from the decomposition

- **ratatui fetched from crates.io once**: the offline registry cache held
  the crate files but not the index entries; one `cargo fetch` (the same
  network path the `trainer` build already documents for `burn`) fixed
  the resolution — after that everything builds offline again.
- **"A minute and a shift"** became "minute + cumulative run-to-date": a
  bench run of 60–120 s never closes an 8 h shift; the shift view is the
  whole run (the escalation's "simplify to a single window" applied
  preemptively, in the naming).
- **The end-of-run signal** is `{node}/end` markers rather than a wall
  clock: the nodes replay faster than real time, and the aggregator must
  close its final window deterministically. A node dying without a marker
  stalls the aggregator (documented; Ctrl-C keeps the CSV written so far).
- **Node A is not broken by gaussian noise** (see sensitivity): the D5
  noise sweep shows −0.000 A error up to σ=0.6 — the expected "tighten the
  noise" escalation does not materialize because the classes are
  amplitude-separated. The honest error channels found: sub-resolution
  episodes (temporal) and the Q shift asymmetry.

## Risks (plan section 11)

- The "too good" risk materialized in the main table (in-distribution) and
  is bounded by the sensitivity tables — the report should lead with
  those, not the zeros.
- The loopback broker is still a test broker: no QoS ≥ 1, no retention, no
  persistent sessions. Fine for the bench; a production line would run
  mosquitto (the client code is unchanged).
- The aggregator's watermark waits for *all* expected nodes; a crashed
  node means no final flush (the minute windows still close; the CSV is
  flushed row-by-row).

## Decision for week 6

Per the plan: QEMU LM3S6965 (flash/RAM from the ELF, UART demo), criterion
benchmarks for the engine ("Conv1D vs the reshape trick"), the report
(lead with the sensitivity story), and the demo recording (the bench
script is the demo scenario). The Q confirmation-threshold study (PR
curve, plan §10) is the natural follow-up to the Q-shift asymmetry found
in D5.

## Retro

- The watermark design paid for itself: the determinism check compares
  full CSVs across runs with three racing publisher threads and passes
  bit-identically (run_id aside) — no sleeps, no retries.
- Three test-authoring bugs were found by the experiment before any
  report was written: underscores inside JSON string literals
  (`"t_ms":10_000` parses as 10), a `[good, cracked]` histogram
  destructured as `[good, total]` (true Q was 26.4 until caught), and a
  boundary test inserting a status out of stream order. The truth-side
  arithmetic deserved the same tests as the measured side — now it has
  them.
- The zero-error table was initially read as a bug; writing the
  sensitivity sweeps reframed it as the correct result for
  in-distribution data. "Where does it break" turned out to be the more
  interesting question than "how big is the error".
