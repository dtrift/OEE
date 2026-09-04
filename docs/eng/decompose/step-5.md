# Week 5 — decomposition (node P + aggregator + dashboard + the main experiment)

> Branch: `feat/aggregator-dashboard` (OEE repo only)

> Breakdown of the "Week 5" row of the plan [`plan.md`](../plan.md), plan section 9.
> Closing the loop with a number: node P (part counting), the OEE aggregator (A×P×Q),
> a ratatui TUI dashboard (fallback — Node-RED) and the main "measured vs true OEE"
> experiment (plan section 10). If Q was dropped by the week 4 cut-line — Quality = 1.0,
> the week's plan does not change. Mode: 1 person; weekdays ~2–3 h, Saturday ~4 h.
> Estimate: ~16–19 h.
>
> Input: the week 4 gate — A statuses (and Q's, if alive) in MQTT, the cut-line decision
> recorded. Already in the repo: `oee-aggregator` with the `oee(A, P, Q)` formula and
> tests (`perfect_line`, `zero_factor_zeroes_oee` — the Q = 1.0 baseline works);
> `WindowSpec` marks P as event-driven (no windows). The belt does not exist in the
> simulator yet — its arrival is D1.
>
> Detailing of the draft [`plan/step-5.md`](../plan/step-5.md): tied to the contracts in
> the repo and the week 4 artifacts; on conflict → this file is edited, the plan only in
> substance.

## Week gate (minimum done)

- [ ] Node P counts parts: jitter, doubles, skips handled; the count matches the truth
      on the baseline run.
- [ ] The aggregator publishes `oee/line1/oee` and the components over windows
      (minute/shift).
- [ ] TUI dashboard (ratatui): live OEE, A, P, Q, a counter, Q verdicts (or a cut-line
      note).
- [ ] The "scenario → true OEE → measured → error" table across 4 scenarios; a repeat
      with one seed → an identical result.

## Day-by-day summary

| Day | Session topic | Essence                                        | Artifact              |
| --- | ------------- | ---------------------------------------------- | --------------------- |
| D0  | infra         | branch, entry conditions (cut-line, fresh log) | a bench run at hand   |
| D1  | simulator+P   | belt in the simulator; edge detector, counting | P count = truth       |
| D2  | aggregator    | subscription, windows, formula over `oee()`    | OEE in MQTT           |
| D3  | dashboard     | ratatui: Gauge/Sparkline + comparison table    | dashboard alive       |
| D4  | experiment    | scenarios: normal/downtime/slowdown/rejects    | raw runs              |
| D5  | analysis      | error table, sensitivity                       | measured vs truth     |
| D6  | robustness    | other seeds; one-command launch                | determinism confirmed |
| D7  | gate          | checklist, retro, week 6 plan                  | `docs/week5-gate.md`  |

## D0 (evening before start, ~0.5 h): entry conditions

1. Branch `feat/aggregator-dashboard`.
2. Re-read `docs/week4-gate.md`: the cut-line decision determines the Q parts of the
   dashboard and the formulas (Quality = 1.0 if Q was dropped).
3. A fresh week 4 bench run — the input data for aggregator development.

## D1 (Mon, ~2–3 h) — simulator: belt; node P: counting

1. The belt in `line-simulator` (plan section 4): parts pass an IR barrier; intervals
   with jitter, sometimes two in a row (anti-double-count), sometimes a skip. IR
   events — into the stream/CSV; ground truth — the count per the scenario.
2. Node P: an edge detector + an anti-double-count window (two events inside a window
   = one part); P is event-driven, no windows (`WindowSpec(P) = None`).
3. Publishing `oee/line1/p/count` (`{count, t_ms, run_id}`).

Check: on a scenario with doubles and skips, the P count = truth.

## D2 (Tue, ~2–3 h) — aggregator: MQTT + windows

1. `mqtt-min` learns to subscribe (the week 5 first task per the week-4 gate):
   SUBSCRIBE/SUBACK (QoS 0) + incoming PUBLISH parsing; the `mqtt_min::testing`
   loopback broker — dispatch to subscribers; tests on the same wire path as
   week 4 (rumqttc is not fetchable offline — week 4's publishing already runs
   on mqtt-min).
2. Subscription to `oee/line1/{a/status, p/count, q/verdict}`; aggregation
   windows: a minute and a shift (plan section 1).
3. Formulas over the window on top of the ready-made `oee()`: A = Run/Planned,
   P = IdealCycle×Count/RT, Q = Good/Total (or 1.0 per the cut-line).
4. Publishing `oee/line1/oee` (the JSON schema is the D3 dashboard contract)
   + a CSV log (raw material for the experiment table).

Check: on a hand-made scenario the components converge with the truth by construction;
a unit test on a fixed window.

## D3 (Wed, ~3 h) — a ratatui TUI dashboard

A new crate `oee-dashboard` (the 6th workspace entity: root + 5 crates), the whole
stack in Rust. Layers:

| Layer                  | What it does                                      | Tests               |
| ---------------------- | ------------------------------------------------- | ------------------- |
| MQTT thread (mqtt-min) | subscribe `oee/line1/#` → an mpsc channel         | the loopback broker |
| `DashboardState`       | pure `on_message(topic, payload)` — all the logic | unit tests          |
| render `ui(f, &state)` | OEE/A/P/Q Gauge, counter, verdicts, Sparkline     | by eye              |

1. (0:00–0:20) the crate skeleton, `ratatui::init()`, hello-world on screen.
2. (0:20–0:50) the MQTT thread on top of the D2 mqtt-min SUBSCRIBE (the week 4
   nodes only publish — they have no subscription) + channel + an "updated N s
   ago" status bar (notices a broker disconnect).
3. (0:50–1:40) `on_message`: JSON `oee/line1/oee` (the schema from D2), zone colors
   (green ≥85%, yellow ≥60% — plan section 1 guidelines), the P counter, a Q verdict
   ticker; a unit test on a corrupt payload — garbage does not take down the display
   (error isolation).
4. (1:40–2:10) an OEE history Sparkline; redraw ~5 fps, `q` to exit;
   `ratatui::restore()` via a panic hook.
5. (2:10–3:00) a full run with the bench, a narrow terminal, buffer.

The "measured vs truth" table — from the aggregator's CSV, we do not drag it into the
TUI.

Check: while the simulator runs, the numbers live and change; the crate's `cargo test`
is green.
Fallback plan: if ratatui stalls for more than ~1.5 h → Node-RED in a container
(widgets without code); the minimal fallback — MQTTX (~15 min, live topic viewing).
Does not affect the gate: the week's main artifact is the table, not the display.

## D4 (Thu, ~3–4 h) — the main experiment

1. Scenarios per plan section 10: normal / downtime / slowdown / rejects; each with
   its own seed. Slowdown — the belt slower than nominal (P < 1); rejects — cracked
   taps (Q < 1), if Q is alive.
2. Auto-collection: run → the aggregator's CSV → the "true OEE → measured → error"
   table; the true one is computed from the scenario (ground truth by construction).
3. Run twice with one seed — an identical result (determinism, plan section 10).

Check: the table is filled in for all four scenarios.

## D5 (Fri, ~2–3 h) — error analysis

1. A breakdown by components, not by the final number: where do we lose — A flap, P
   double counting, the Q threshold?
2. Sensitivity: simulator noise, the P detector threshold, A hysteresis.
3. Materials for the report: the table + 2–3 observations.

## D6 (Sat, ~3 h) — robustness and polish

1. Other seeds: errors within reasonable bounds, the conclusions do not flip.
2. README: launching the whole bench with one command (a script: mosquitto + simulator
   + nodes + `cargo run -p oee-dashboard`) — the same thing is the week 6 demo
   scenario.
3. Buffer for the week's unfinished items.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; the measured vs truth table is the main artifact.
2. Retro: hours; what to carry into week 6.
3. Week 6 plan: QEMU, benchmarks, report, demo.

Artifact: `docs/{rus,eng}/week5-gate.md` (a rus/eng pair — the week 3–4
convention; in the early plan — `tmp/OEE/week5-gate.md`; `tmp/` is gitignored,
precedent — week 1).

## Escalation points

- Measured OEE strongly ≠ true → cross-check by components (A, P, Q separately): most
  often the P anti-counting window or the A windows are to blame — fix it there.
- ratatui stalls → the Node-RED / MQTTX fallback plan (see D3); the gate is not
  blocked.
- The scenarios give "perfect" 0% error → tighten the noise/jitter, otherwise the
  table is unconvincing (the "too good" risk, plan section 11).
- The aggregator drowns in window states → simplify to a single window (minute),
  compute the shift after the fact from the CSV; keep only the minute one live.

## Anti-scope (what we do NOT do in week 5)

- QEMU, footprint, criterion — week 6.
- Grafana — future work (plan section 12).
- New simulator features — only if needed for the experiment's credibility.
