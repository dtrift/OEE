# Week 4 Gate — Nodes A and Q End-to-End + Cut-Line

> Implemented 2026-09-03 per [decompose/step-4.md](decompose/step-4.md) on
> branch `feat/nodes-a-q` (OEE only; the fork is untouched). All checks
> re-run at formalization time: workspace — 17 lib + 6 integration suites,
> clippy clean (the fork's pre-existing `mismatched_lifetime_syntaxes`
> warnings stay silenced by the CI flag); `cargo test -p nodes
> --test node_pipeline` green.

## Gate checklist

| Gate item                                                               | Status | Artifact / check                                                                                                                                                                                                                         |
| ----------------------------------------------------------------------- | ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Node A statuses in MQTT follow the scenario (idle/run/jam/overload)     | yes    | [node_pipeline.rs](../../nodes/tests/node_pipeline.rs) — `node_a_statuses_match_truth_away_from_boundaries`: deep probes inside every truth interval match; the offline CSV rows sit at the event times + ≤ 240 ms (window + hysteresis) |
| Node A offline-CSV statuses match the ground truth                      | yes    | `a,base42,...` rows against `scenarios/base.toml` events: run 2159 (event 2000), jam 20159, run 20559, overload 40159, run 40959, idle 58159                                                                                             |
| Node Q verdicts in MQTT / offline CSV: pass-crack per tap (or cut-line) | yes    | `node_q_verdicts_match_the_tap_truth`: 282/282 taps, accuracy ≥ 0.98 asserted (1.00 observed on seed 42); Q kept — see the cut-line                                                                                                      |
| Nodes survive bad data / a dead broker (error isolation)                | yes    | `node_a_survives_a_corrupt_window` (128 corrupted rows → windows dropped, node alive); `dead_broker_degrades_to_offline_without_panicking` + the CLI smoke with `--mqtt 127.0.0.1:1` (283 failed publishes, offline CSV intact)          |
| The cut-line decision recorded with a reason                            | yes    | Q **stays**: trained through the same rust-ml pipeline (`--task q`), val float/int8 1.0000, ≥ 0.98 on unseen seeds, microflow-vs-interp argmax parity — no crutches found                                                                |

## What was built (per day of the decomposition)

- **D0 — environment**: the `mqtt-min` crate (see Deviations) instead of
  `rumqttc`; the broker check is a user action (mosquitto is not in the
  sandbox). `nodes` gained the `node` binary CLI (`--kind a|q --input …
  --offline … [--mqtt host:port]`).
- **D1 — node A offline**: `nodes/src/sim_source.rs` (`SimSource` over the
  run CSV, `SensorSource` implementation), `nodes/src/status.rs`
  (non-overlapping window assembly, the 2-window anti-flap hysteresis, the
  `node,run_id,t_ms,state` CSV log), `a::run_a` — the full stream → statuses
  pipeline with per-window error isolation.
- **D2 — node A MQTT**: `nodes/src/mqtt_sink.rs` — `oee/line1/a/status`
  JSON `{state,t_ms,run_id}` on confirmed changes only, `…/a/meta` once at
  startup; lazy connect with capped linear backoff; a dead broker degrades
  to offline-only, never panics.
- **D3 — tap channel**: `line-simulator/src/taps.rs` + the `[taps]` scenario
  section (validated, defaults pinned): damped-sine synthesis at 16 kHz
  (good 2.4 kHz/τ14 ms, cracked 1.5 kHz/τ6 ms + rattle, seeded amplitude/
  frequency wander), taps flow only in `Run`; outputs the training dataset
  `label,state,x000..x1023` and the meta `t_ms,verdict`;
  `scenarios/taps.toml`. Determinism: same seed → bit-identical windows.
- **D4 — node Q**: the trainer generalized to a task spec (`TaskSpec`,
  runtime dims — burn's `Module` derive does not carry const generics);
  `--task q` trains `ml/models/model_q.tflite` (11216 bytes, sha256 pinned
  in `model_q.metrics.txt`, confusion 121/121 + 50/50 on val);
  `nodes/src/q.rs` — `#[model]` through the bridge, one window per tap,
  `q::run_q` → `oee/line1/q/verdict`.
- **D5 — integration**: both nodes in one process over the loopback broker
  (`both_nodes_one_run_with_mqtt`); interp-vs-microflow argmax parity on
  the Q model; a predict-latency smoke (100 A + 20 Q windows well under the
  line tempo, even in debug).
- **D6 — cut-line**: see above — Q stays.
- **D7 — this gate.**

## Deviations from the decomposition

- **No `rumqttc`, no mosquitto in the offline sandbox** (the crate is not in
  the registry cache; the broker binary is absent): MQTT is implemented as
  `mqtt-min` — a minimal MQTT 3.1.1 client subset (CONNECT/PUBLISH QoS 0/
  PINGREQ, std TCP, ~230 lines) with the wire format pinned by spec-derived
  unit tests and an in-process loopback broker (`mqtt_min::testing`)
  exercising the full path. The same client runs against a real mosquitto
  unchanged. User actions, in order:
  1. `mosquitto -v` (or the docker service), manual `mosquitto_pub/sub` on
     `oee/line1/#`;
  2. `cargo run -p nodes --bin node -- --kind a --input tmp/run_base42.csv
     --offline tmp/status_a.csv --mqtt 127.0.0.1:1883` and the node Q
     counterpart — then `mosquitto_sub -t 'oee/line1/#' -v` shows the
     statuses/verdicts and the meta lines.
- **The trainer is task-parameterized, not duplicated**: one `ModelCnn`
  over runtime `TaskSpec` dims (A 128→4, Q 1024→2) — const generics are not
  carried by burn's `derive(Module)`. The A path is byte-for-byte the old
  pipeline (same artifacts regenerate).
- **"WAV-like buffers" are CSV**: the tap windows live in the dataset CSV
  (linear floats, the trainer's format); no binary WAV container — the
  trainer and the node read the same file.
- **Node Q timestamps come from the meta CSV** (row-order alignment): the
  dataset schema (`label,state,x…`) is pinned by the trainer and carries no
  time column. A dropped row shifts the alignment by one tap — acceptable,
  the node logs dirty windows.
- The hysteresis is 2 windows (~160 ms) — the "2–3 windows" of D2, item 2.

## Risks (plan section 11)

- "Too-good models" — Q val accuracy is 1.0000 (synthetic taps, seeded
  parameter wander ±8%/±15%, crack rattle 4× noise). The risk is real but
  bounded by design: the scenario carries the drift knobs, and the final
  honesty check is the week-5 measured-vs-truth table on unseen seeds.
- Node A boundary lag: a status change is confirmed ~160–240 ms after the
  true transition (window + hysteresis) — visible in the offline CSV and
  accounted for in the truth comparison (deep probes only). Report material,
  not a defect.
- No new fork risks: the fork is untouched this week.

## Decision for week 5

Per the plan: node P (IR barrier → counting), the OEE aggregator
(A × P × Q over MQTT), the ratatui dashboard, and the measured-vs-truth
table (the main numerical result). Inputs ready: the topic layout
(`oee/line1/{a,q}/…`), the loopback broker for tests, both node pipelines.
The aggregator will need SUBSCRIBE in `mqtt-min` (QoS 0) — the natural
first D-task of week 5.

## Retro

- Hours: not tracked per session (the recurring gap — same note as weeks
  2–3). The implementation pass was effectively one long session.
- The loopback broker initially served connections sequentially — both nodes
  hold their connections open concurrently, so node Q starved until node A
  finished. Lesson: a broker is concurrent by definition; test doubles must
  copy that, not just the packet format.
- `burn`'s `derive(Module)` rejecting const generics cost one redesign
  round (const-generic model → runtime-dims model). The retry was cheaper
  than fighting the derive: runtime dims also removed the turbofish noise
  from the pipeline calls.
- The tap synthesis landed almost on the first try: the physics (ring
  frequency + decay) chosen from the spectrum intuition held up — the Q
  model separates the classes without feature engineering.
