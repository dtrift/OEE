# Week 4 — decomposition (nodes A and Q end-to-end + cut-line)

> Branch: `feat/nodes-a-q` (OEE). The fork is frozen: an inference bug gets a hotfix
> branch, the main line does not wait.

> Breakdown of the "Week 4" row of the plan [`plan.md`](../plan.md), plan section 9.
> Both tracks are integration: node A (sim → feature analysis → predict → MQTT) and node
> Q (sound synthesis + model + integration). The end of the week is the **cut-line**: Q
> not ready → it is dropped, Quality = 1.0 baseline (plan section 9). Mode: 1 person;
> weekdays ~2–3 h, Saturday ~4 h. Estimate: ~16–19 h.
>
> Input: the week 3 gate — `#[model]` inference of `model_a.tflite` on the host, parity
> green, the `nodes → fork/microflow` path dependency already wired up (week 3, D6).
> Contracts in the repo: `SensorSource` (`nodes/src/source.rs`, no_std), `WindowSpec`
> (`features-cli`: A = 128 @ 1.6 kHz; Q = 1024 @ 16 kHz — preliminary, "to be fixed by
> the week 4 lab" — this week is that lab; P — event-driven), the `capture` schema.
>
> Detailing of the draft [`plan/step-4.md`](../plan/step-4.md): tied to the contracts in
> the repo and the week 3 artifacts; on conflict → this file is edited, the plan only in
> substance.

## Week gate (minimum done)

- [ ] Node A statuses in MQTT change according to the scenario (idle/run/jam/overload);
      the offline-CSV statuses match the ground truth.
- [ ] Node Q statuses in MQTT: pass/fail by the tap test (or the cut-line decision).
- [ ] The nodes do not crash: a corrupt window/MQTT disconnect is contained locally,
      the node lives on (error isolation).
- [ ] The cut-line decision is written into `docs/week4-gate.md` with the reason.

## Day-by-day summary

| Day | Session topic | Essence                                      | Artifact                     |
| --- | ------------- | -------------------------------------------- | ---------------------------- |
| D0  | infra         | branch, mosquitto broker, rumqttc            | `mosquitto_sub` responds     |
| D1  | node A        | `SimSource` + windows + predict, offline-CSV | A statuses = truth           |
| D2  | node A        | MQTT: `oee/line1/a/*`, hysteresis            | A statuses in the broker     |
| D3  | simulator     | tap-sound synthesis (intact/cracked)         | sound buffers + CSV metadata |
| D4  | node Q        | model Q + integration + MQTT                 | Q verdicts in the broker     |
| D5  | integration   | both nodes in one run, robustness            | a coherent run, no crashes   |
| D6  | cut-line      | decision on Q + buffer                       | decision recorded            |
| D7  | gate          | checklist, retro, risks, week 5 plan         | `docs/week4-gate.md`         |

## D0 (evening before start, ~0.5–1 h): environment

1. Branch `feat/nodes-a-q` in OEE.
2. Broker: mosquitto locally (docker or a service); a manual `mosquitto_pub/sub` check
   on `oee/line1/#`.
3. `rumqttc` into `[workspace.dependencies]` of the root `Cargo.toml` (pin the version;
   the week 5 dashboard will use the same one).

## D1 (Mon, ~3 h) — node A: the offline pipeline

1. `SimSource`: an implementation of `SensorSource` over the simulator's run CSV (the
   run file is the source; a live stream is not needed until week 5). `Sample = f32`
   amperes.
2. Windows per `WindowSpec(A)` (128 @ 1.6 kHz, 80 ms) → `predict()` (the week 3 model
   via `#[model]` in `nodes`) → status.
3. Offline mode: the node writes statuses to a CSV (`node,run_id,t_ms,state` — the
   `capture` schema); debugging without a broker.

Check: A statuses on the `scenarios/base.toml` run match the ground truth (the `state`
column); mismatches — only at window boundaries.

## D2 (Tue, ~2–3 h) — node A: MQTT

1. Publishing (rumqttc): `oee/line1/a/status` — JSON `{state, t_ms, run_id}`;
   `oee/line1/a/meta` — model version, `WindowSpec`. On status change, not every tick.
2. Hysteresis/averaging over 2–3 windows at the class boundary (anti-flap).
3. Broker disconnect: reconnect with backoff, the node keeps computing and appends to
   the offline-CSV.

Check: smoke — simulator | node A | `mosquitto_sub -t oee/line1/a/#`; statuses change
according to the scenario.

## D3 (Wed, ~3 h) — simulator: tap-sound synthesis

1. Tap events in the scenario: every part gets tapped; pass/fail is known — ground
   truth (plan section 4).
2. Synthesis: a decaying sinusoid; intact — a higher frequency, longer decay; cracked —
   lower, faster + noise; parameters varied by seeded RNG.
3. The 16 kHz rate = `WindowSpec(Q)`: a window of 1024 @ 16 kHz = 64 ms. The week 4
   lab: confirm the window with the tap spectrum; if 64 ms is not enough — it is fixed
   only in `window_spec` (the single point of truth).
4. Export: WAV-like buffers + CSV metadata (`t_ms, verdict`) for training Q.

Check: the classes are distinguishable by spectrogram/by ear; the seed is
reproducible; the determinism test extended to the tap channel.

## D4 (Thu, ~3–4 h) — node Q: model and integration

1. The tap dataset (a pass/fail mix, several seeds) → model Q: the same Conv1D family
   as A, input `(1024, 1)` → int8 → `ml/models/model_q.tflite`.
2. Node Q: tap buffer → window → `predict()` → `oee/line1/q/verdict`
   (`{verdict, t_ms, run_id}`); offline-CSV — like A.
3. A rough confusion matrix for Q on val.

Check: Q verdicts in the broker match the truth on clear-cut cases.

## D5 (Fri, ~2–3 h) — integration and robustness

1. A coherent run: simulator (current + taps) | node A | node Q | broker.
2. Error isolation: a corrupt window/empty buffer — a warning and a skip, the node
   lives on (rule: a `raise`/panic in a thread does not take down the whole node).
3. A pace estimate: predict keeps up with the belt pace (not a metric — "no worse
   than").

Check: a run without crashes; A statuses and Q verdicts in the broker.

## D6 (Sat, ~3 h) — CUT-LINE: the decision on Q

1. Check: did Q pass D4–D5 without crutches? yes → keep it; no → drop it, Quality =
   1.0, OEE = A×P (the week 5 aggregator is already ready for the baseline — the
   `zero_factor_zeroes_oee` test).
2. The reason for the decision → `docs/week4-gate.md` — a trade-off for the report.
3. Buffer: finish off whatever slipped during the week.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; every "yes" — backed by a link to an artifact.
2. Retro: hours vs estimate; half the course is done — update the risks (plan
   section 11).
3. Week 5 plan: node P + aggregator + dashboard + the main experiment.

Artifact: `docs/week4-gate.md` (in the early plan — `tmp/OEE/week4-gate.md`;
`tmp/` is gitignored, precedent — week 1).

## Escalation points

- The MQTT environment eats time → the nodes must work offline (CSV, D1) — the broker
  goes on top; do not look at the dashboard until week 5.
- The acoustics act up (model Q is bad) → move the cut-line before D6, do not drag it
  out; the freed-up days go to polishing A.
- A statuses flap at the class boundary → hysteresis/averaging (D2, item 2); the fact —
  into the report (threshold sensitivity).
- A fork bug surfaces (inference/parity) → a hotfix branch in the fork + the question:
  why did the week 3 parity test miss it; close the hole with a test.

## Anti-scope (what we do NOT do in week 4)

- Node P (part counting) — week 5; the OEE aggregator and dashboard — week 5.
- QEMU — week 6; benchmarks — week 6.
- Grafana and niceties — future work / the remainder of week 6.
- A live stream simulator → nodes (instead of CSV) — only if week 5 needs it.
