# Week 5 — decomposition (node P + aggregator + the main experiment)

> Branch: `feat/aggregator-dashboard` (OEE repo only)

> Decomposition of the "Week 5" row from [`plan.md`](../plan.md),
> section 9. Closing with numbers: node P (part counting), the OEE aggregator (A×P×Q), a ratatui
> TUI dashboard (the fallback plan — Node-RED) and the main "measured vs true OEE" experiment (section 10). If Q was dropped by the cut-line —
> Quality = 1.0, the week's plan doesn't change. Mode: 1 person; weekdays ~2–3 h, Saturday
> ~4 h. Estimate: ~16–19 h. Entry: the statuses of A (and Q, if alive) in MQTT.

## Week gate (minimum ready)

- [ ] Node P counts parts: jitter, doubles, misses handled.
- [ ] The aggregator publishes OEE and the components over windows (minute/shift).
- [ ] The TUI dashboard (ratatui): live OEE, A, P, Q, a counter, Q verdicts.
- [ ] The "scenario → true OEE → measured → error" table is assembled.

## Day-by-day summary

| Day | Session topic | Summary                                         | Artifact                |
| --- | ------------- | ----------------------------------------------- | ----------------------- |
| D1  | node P        | IR edge detector + anti-double-count            | the count matches truth |
| D2  | aggregator    | subscription, windows, the A×P×Q formula        | OEE in MQTT             |
| D3  | dashboard     | ratatui: Gauge/Sparkline + the comparison table | the dashboard is alive  |
| D4  | experiment    | scenarios: normal/downtime/slowdown/fail        | raw runs                |
| D5  | analysis      | the error table, sensitivity                    | measured vs truth       |
| D6  | robustness    | other seeds, a repeat = identical               | determinism confirmed   |
| D7  | gate          | checklist, retro, the week 6 plan               | `tmp/OEE/week5-gate.md` |

## D1 (Mon, ~2–3 h) — node P: counting

1. IR barrier events from the simulator (section 4): interval jitter, sometimes two
   parts in a row, sometimes a miss.
2. An edge detector + an anti-double-count window (two events inside the window = one).
3. Ground truth: the scenario's count vs the node's count — a match on the baseline run.

Check: on a scenario with doubles and misses the P count is correct.

## D2 (Tue, ~2–3 h) — aggregator: the OEE formula

1. A subscription to `oee/line1/*`; aggregation windows: a minute and a shift (section 1).
2. Formulas: A = Run/Planned, P = IdealCycle×Count/RT, Q = Good/Total (or 1.0).
3. Publishing `oee/line1/oee` + the components; a CSV log for the report.

Check: on a manual scenario the components match the truth by construction.

## D3 (Wed, ~3 h) — the TUI dashboard on ratatui

A new crate `oee-dashboard` (the 6th in the workspace), the whole stack stays in Rust. Layers:

| Layer                  | What it does                                      | Tests                |
| ---------------------- | ------------------------------------------------- | -------------------- |
| MQTT stream (rumqttc)  | a subscription `oee/line1/#` → an mpsc channel    | reuse from the nodes |
| `DashboardState`       | pure `on_message(topic, payload)` — all the logic | unit tests           |
| `ui(f, &state)` render | OEE/A/P/Q Gauge, counter, verdicts, Sparkline     | by eye               |

1. (0:00–0:20) the crate skeleton, `ratatui::init()`, hello-world on screen.
2. (0:20–0:50) the MQTT stream (copy-paste of the subscription from the week 4 node) + channel + a status bar
   "updated N s ago" (it notices a broker outage).
3. (0:50–1:40) `on_message`: JSON `oee/line1/oee` (the schema from D2), zone colors (green
   ≥85%, yellow ≥60%), the P counter, the Q verdict strip; a unit test for a bad payload —
   garbage must not crash the display (error isolation).
4. (1:40–2:10) an OEE history Sparkline; a redraw on a ~5 fps tick, `q` — quit;
   `ratatui::restore()` via a panic hook.
5. (2:10–3:00) a full run with the bench, a narrow terminal, buffer.

The "measured vs truth" table — from the aggregator's CSV, we don't drag it into the TUI.

Check: while the simulator runs the numbers live and change; the crate's `cargo test` is green.

Fallback plan: ratatui drags on longer than ~1.5 h → Node-RED in a container (the old D3
variant, widgets without code); the minimal fallback — MQTTX (~15 min, live topic viewing).

## D4 (Thu, ~3–4 h) — the main experiment

1. Scenarios per section 10: normal / downtime / slowdown / fail; each with its own seed.
2. Auto-collection: run → CSV → the "true OEE → measured → error" table.
3. Run twice with the same seed — an identical result (determinism, section 10).

Check: the table is filled in for all four scenarios.

## D5 (Fri, ~2–3 h) — error analysis

1. Dissecting the errors by component (not only the total): where do we lose — A flapping,
   P double counting, the Q threshold?
2. Sensitivity: the noise in the simulator, the P detector threshold, the A hysteresis.
3. Material for the report: the table + 2–3 observations.

## D6 (Sat, ~3 h) — robustness and polish

1. Other seeds: the errors stay within reasonable bounds, the conclusions don't flip.
2. Polish: a README on launching the whole bench with one command.
3. A buffer for the week's loose ends.

## D7 (Sun, ~2 h) — gate and retro

1. The "Week gate" checklist; the measured vs truth table — the main artifact.
2. Retro: hours; what to carry into week 6.
3. The week 6 plan: QEMU, benchmarks, the report, the demo.

Artifact: `tmp/OEE/week5-gate.md`.

## Escalation points

- The measured OEE is far ≠ the true one → cross-check by components (A, P, Q separately),
  not by the final number; most often the P anti-counting window or the A windows are to blame.
- ratatui drags on → the fallback plan: Node-RED in a container (~1 h, widgets without code);
  the minimal fallback — MQTTX (live viewing of `oee/line1/#`, ~15 min). It doesn't affect
  the gate: the week's main artifact is the measured vs truth table, not the display.
- The scenarios give "perfect" 0% error → tighten the noise/jitter, otherwise the table
  is unconvincing (the "too good" risk, section 11).

## Anti-scope (what we do NOT do in week 5)

- QEMU, footprint, criterion — week 6.
- Grafana — future work (section 12).
- New simulator features — only if they are needed for the experiment's credibility.
