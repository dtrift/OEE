# The demo recording scenario (week 6, D6)

> Target length: 2–4 minutes, one take + a backup take. Everything below is
> one-command reproducible from a clean clone (see section "Prep"). Record
> a full-screen terminal (OBS or the platform's recorder), 1080p, dark
> theme.
>
> A recorded run already exists for the 1M soak variant:
> [`../media/OEE-bench-1m.mp4`](../media/OEE-bench-1m.mp4) — the dashboard
> winding up to ~1.07M messages. It is that variant's take, not the
> full four-scene scenario below.

## Prep (before hitting record)

```bash
# from a clean clone:
cargo build --release                        # ~all crates
docker build -t oee-qemu qemu/               # or: apt install qemu-system-arm
cargo test --workspace --release            # optional, shows green — 10 s of B-roll
scripts/qemu-parity.sh                      # ends with "PARITY OK" on screen
```

Have three terminal tabs ready (same working directory):

1. `scripts/qemu-parity.sh` (the portability beat)
2. `scripts/bench.sh scenarios/week5/normal.toml 42` (the twin beat)
3. an editor open at `scenarios/week5/rejects.toml` (the change beat)

## The script (scene by scene)

| # | Scene (seconds) | What to say / show                                                                                                                                                                                                                                                                                                                                    |
| - | --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0 | 0:00–0:20       | The one-sentence pitch (plan section 0, short version): a digital twin of a line measuring OEE with three TinyML nodes; the engine contribution is Conv1D in a MicroFlow fork. Show the repo tree briefly.                                                                                                                                            |
| 1 | 0:20–0:50       | **Portability**: run tab 1. Point at the UART lines: the firmware on the emulated LM3S6965 (Cortex-M3) classifies 4 windows; the host prints the same lines; the script's diff ends in `PARITY OK: 4 windows, bit-for-bit`. One sentence: same int8 model, same bits, two ISAs.                                                                       |
| 2 | 0:50–2:00       | **The twin**: run tab 2. The bench generates the scenario, then the dashboard opens and the gauges fill live as the nodes replay (a few seconds), freezing on the final window; walk through the gauges (OEE/A/P/Q with the 85%/60% zones), the part counter, the machine status, the verdict ticker. Name the numbers on screen vs the week-5 table. |
| 3 | 2:00–2:40       | **The change**: quit the dashboard (`q`), edit `rejects.toml` on screen (bump the crack probability, e.g. 0.10 → 0.50), rerun `scripts/bench.sh scenarios/week5/rejects.toml 42`. The Q gauge and OEE drop before the viewer's eyes. One sentence: availability stayed, quality fell — the twin sees the line, not the code.                          |
| 4 | 2:40–3:00       | **Wrap**: the report pointer (`docs/eng/report.md`): correctness table, 1.67–1.73× kernel speedup, 45.3 KiB flash footprint. End on the repo README.                                                                                                                                                                                                  |

## Notes for the take

- The dashboard opens BEFORE/DURING the node replay on purpose: the bench
  broker is QoS 0 without retention, so it cannot replay the past to a late
  subscriber. During the few seconds of the replay the gauges update live;
  after the end markers they freeze on the final window — that frozen state
  is what scene 2 walks through.
- The dashboard needs a terminal ≥ 80×24; node progress goes to
  `tmp/bench/{a,p,q}.log` (tailed by the script after `q`).
- Do not narrate build output — cut from command to result.
- If a take must be shortened: drop scene 1 to its last 10 seconds (the
  `PARITY OK` line) — scenes 2–3 are the core.

## The backup take

Scenes 2–3 only (the twin + the reject change), plus the `PARITY OK` tail
of scene 1. Under 90 seconds.
