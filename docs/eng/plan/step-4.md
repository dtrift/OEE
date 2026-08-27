# Week 4 — decomposition (nodes A and Q end-to-end) + cut-line

> Branch: `feat/nodes-a-q` (OEE repo only)

> Decomposition of the "Week 4" row from [`plan.md`](../plan.md),
> section 9. Both lines are integration work: node A (sim → features → predict → MQTT) and node Q
> (sound synthesis + model + integration). The end of the week is the **cut-line**: if Q isn't ready,
> it gets dropped, Quality = 1.0 baseline (plan section 9). Mode: 1 person;
> weekdays ~2–3 h, Saturday ~4 h. Estimate: ~16–19 h. Entry: `#[model]` inference on the host,
> model A trained.

## Week gate (minimum ready)

- [ ] Node A statuses in MQTT change according to the simulator scenario (idle/run/jam).
- [ ] Node Q statuses in MQTT: pass/fail by the tap test (or the cut-line decision).
- [ ] The nodes don't crash: bad data / an MQTT outage are absorbed point-wise (error isolation).
- [ ] The cut-line decision is recorded in `tmp/OEE/week4-gate.md` with the reason.

## Day-by-day summary

| Day | Session topic | Summary                                         | Artifact                 |
| --- | ------------- | ----------------------------------------------- | ------------------------ |
| D1  | node A        | `SensorSource`/`SimSource` + features + predict | node A in offline mode   |
| D2  | node A        | MQTT: `oee/line1/a/status`                      | A statuses in the broker |
| D3  | simulator     | tap sound synthesis (intact/cracked)            | sound buffers + CSV      |
| D4  | node Q        | the Q model + integration + MQTT                | Q statuses in the broker |
| D5  | integration   | both nodes in one run, robustness               | a coherent run           |
| D6  | cut-line      | the decision on Q + buffer                      | decision recorded        |
| D7  | gate          | checklist, retro, the week 5 plan               | `tmp/OEE/week4-gate.md`  |

## D1 (Mon, ~3 h) — node A: the pipeline

1. The `SensorSource` trait + `SimSource` — architectural insurance for the hardware (section 2).
2. Windows over the simulator stream → features (`features-cli`) → `predict()` (the week 3 model).
3. Offline mode: without MQTT the node writes statuses to CSV — debugging without a broker.

Check: the A statuses in CSV match the scenario ground truth.

## D2 (Tue, ~2–3 h) — node A: MQTT

1. A local mosquitto broker (docker); the topics `oee/line1/{a/status,meta}`.
2. Publishing (rumqttc) on status change; the node survives a broker outage.
3. Smoke test: simulator | node A | `mosquitto_sub` in the console.

Check: the statuses in the broker change according to the scenario.

## D3 (Wed, ~3 h) — simulator: tap sound synthesis

1. Per section 4: a decaying sine; intact — a higher frequency, a longer decay;
   cracked — a lower frequency, faster + noise; the parameters are varied by seeded RNG.
2. Tap events in the scenario: every part knocks, its pass/fail is known (ground truth).
3. Export: WAV-like buffers + CSV metadata for node Q training.

Check: the sound classes are distinguishable by ear / by spectrogram; the seed is reproducible.

## D4 (Thu, ~3–4 h) — node Q: model and integration

1. The tap dataset → sound-window features → a Conv1D architecture like A's → int8
   → `model_q.tflite`.
2. Node Q: receive a tap buffer → features → predict → MQTT `oee/line1/q/verdict`.
3. A rough accuracy check on val (a draft confusion matrix).

Check: the Q verdicts in the broker match the truth on clear-cut cases.

## D5 (Fri, ~2–3 h) — integration and robustness

1. One run: simulator | node A | node Q | broker — a coherent session.
2. Error isolation: a bad window / an empty buffer — a warning and a skip, the node lives on.
3. A rough check: predict keeps up with the belt pace (not a metric, just "no worse").

## D6 (Sat, ~3 h) — CUT-LINE: the decision on Q

1. Check: did Q pass D4–D5 without hacks? yes → keep it; no → drop it,
   Quality = 1.0, OEE = A×P (per the plan).
2. Record the reason for the decision in `week4-gate.md` — it is a trade-off for the report.
3. Buffer: finish what slipped during the week.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; links to artifacts.
2. Retro: hours vs estimate; half the course is done — update the risks (section 11).
3. The week 5 plan: node P + the aggregator + the main experiment.

## Escalation points

- The MQTT environment eats time → the nodes must work offline (CSV), the broker
  comes on top; don't look at the dashboard until week 5.
- The acoustics misbehave (the Q model is bad) → the cut-line earlier than D6, don't drag
  it out: the freed days go to polishing A.
- The A statuses flap at the class boundary → hysteresis / averaging over 2–3 windows;
  the fact — into the report (threshold sensitivity).

## Anti-scope (what we do NOT do in week 4)

- Node P (part counting) — week 5; the OEE aggregator and dashboard — week 5.
- QEMU — week 6; benchmarks — week 6.
- Grafana and eye candy — future work / the remainder of week 6.
