# The soak scenarios: 100 k and 1 M messages through the bench

> [`scenarios/soak.toml`](../../scenarios/soak.toml) — the message-load
> scenario (week 6): the same densities as
> [`scenarios/week5/normal.toml`](../../scenarios/week5/normal.toml),
> stretched to 3 h of simulated time (~100 000 messages).
> [`scenarios/soak-1m.toml`](../../scenarios/soak-1m.toml) — ~12 h with
> the belt/tap periods at 150 ms: ~1 072 500 messages (~2.2 GB of
> artifacts: mind the disk). Soak tests of the bench broker and the
> aggregator, not demo scenarios. All numbers below are from verified
> runs (seed 42).

## What to expect (measured, seed 42)

| Metric                                        | soak.toml (3 h)                                 | soak-1m.toml (12 h, 150 ms)                       |
| --------------------------------------------- | ----------------------------------------------- | ------------------------------------------------- |
| messages on `oee/line1/#` (dashboard counter) | 99 787                                          | 1 072 502                                         |
| node A / P / Q publishes                      | 9 / 24 241 / 25 465                             | 24 / 260 876 / 274 634                            |
| aggregator                                    | 49 712 received, 50 072 republished, 0 errors   | 535 534 received, 536 968 republished, 0 errors   |
| minute windows                                | 180 (+ the shift scope; 361 CSV rows)           | 717 (+ the shift scope)                           |
| node A status rows (flap check)               | 7                                               | 23                                                |
| OEE                                           | 0.853 (A 0.943, P 0.952, Q 0.950), 24 239 parts | 0.906 (A 0.954, P 1.000*, Q 0.950), 260 875 parts |
| stream generation                             | 25 s                                            | ~35 s                                             |
| whole replay, release binaries                | 13 s                                            | ~1.5–2 min                                        |
| artifacts                                     | ~585 MB (`run.csv` ~337 MB, `taps.csv` ~248 MB) | ~2.2 GB (`run.csv` ~1.35 GB, `taps.csv` ~0.99 GB) |

\* P clamps to 1.000: the bench aggregator's ideal cycle stays 400 ms
(`scripts/bench.sh`) while parts arrive every ~143 ms — for a meaningful
P run the aggregator with `--ideal-cycle-ms 150`. A and Q stay meaningful
as-is.

## The 30-hour wall (why soak-1m is 12 h, not 30)

The naive 1M scaling — 30 h at the default densities — was built and run
first, and it broke honestly: the simulator's carrier phase is computed
in f32 seconds (`sin(2π·f·t)`, `line-simulator/src/signal.rs`), and the
phase error grows with t. Measured on the 30 h run: node A flapped
2–6 times per hour through hour 13, then 600–4 800 times per hour from
hour 14 on (36 523 status rows total; the measured Availability collapsed
to 0.591 against a ~0.92 truth — the classifier was reading phase noise,
not the envelope). Twelve hours stays inside the clean window; the missing
message volume comes from density (belt/tap periods 400 → 150 ms). A
proper fix — f64 phase accumulation — would change the generated signal
bit-for-bit and invalidate the trained models and the week-5 record, so
it is pinned as future work, not a week-6 change.

The machine-state events are spread over the whole timeline on purpose:
node A publishes only on confirmed status changes, so bunched events would
silence it for hours (P and Q keep streaming regardless — they dominate
the count).

## Launch 1: the one-command way

```bash
RELEASE=1 scripts/bench.sh scenarios/soak.toml 42     # release: replay ~13 s
```

The script generates the streams (~585 MB into `tmp/bench/`), starts the
broker and the aggregator, opens the dashboard and replays the nodes in
the background. The message counter winds up to ~99 800, then "stream
ended"; the gauges freeze on the 3-hour values (OEE 85.3%, A 94.3%,
P 95.2%, Q 95.0%, parts 24 239). Exit with `q`.

Without `RELEASE=1` the script builds **debug** binaries (the right
trade-off at the 60 s demo scale) — and at this CSV size the nodes run
sequentially, so expect ~3 min of node A silence before P's burst and
then ~10–15 min of node Q: the dashboard is NOT hung, it is waiting
between the rare debug publishes.

## Launch 2: the fast way (release, ~13 s replay)

```bash
cargo build -q --release -p mqtt-min -p nodes -p oee-aggregator -p oee-dashboard

PORT=18845; ADDR=127.0.0.1:$PORT; OUT=tmp/soak
./target/release/broker $PORT >"$OUT/broker.log" 2>&1 &
./target/release/aggregator --mqtt $ADDR --ideal-cycle-ms 400 \
    --out "$OUT/oee_windows.csv" >"$OUT/aggregator.log" 2>&1 &
sleep 0.5
( sleep 1
  ./target/release/node --kind a --input "$OUT/run.csv"   --offline "$OUT/statuses.csv" --mqtt $ADDR --run-id soak-42 >"$OUT/a.log" 2>&1
  ./target/release/node --kind p --input "$OUT/ir.csv"    --offline "$OUT/counts.csv"   --mqtt $ADDR --run-id soak-42 >"$OUT/p.log" 2>&1
  ./target/release/node --kind q --input "$OUT/taps.csv"  --meta "$OUT/taps_meta.csv" \
      --offline "$OUT/verdicts.csv" --mqtt $ADDR --run-id soak-42 >"$OUT/q.log" 2>&1
) &
./target/release/oee-dashboard --mqtt $ADDR
```

The one-second head start lets the dashboard subscribe before the first
publish (the broker has no retention). After `q` on the dashboard, stop
the background jobs — and check for orphaned brokers if you skip this
step or close the terminal first (they hold their port but idle
harmlessly otherwise):

```bash
kill %1 %2 2>/dev/null
pgrep -af 'target/release/broker' || echo "no orphan brokers"
```

For a fully clean run, regenerate the streams first (release simulator —
a few seconds):

```bash
mkdir -p tmp/soak
./target/release/line-simulator --scenario scenarios/soak.toml --seed 42 \
    --out tmp/soak/run.csv --taps-dataset tmp/soak/taps.csv --taps-meta tmp/soak/taps_meta.csv \
    --belt-events tmp/soak/ir.csv --belt-meta tmp/soak/belt_meta.csv
```

## Cleanup

The soak artifacts weigh ~585 MB per output directory:

```bash
rm -rf tmp/soak tmp/bench
```

## Why debug is slow here

Node A parses a 337 MB CSV of 135 000 windows: 184 s in a debug build
against 6 s in release — the inference itself is negligible (135 000 ×
~15 µs ≈ 2 s). At the demo scale (60 s scenarios) the debug builds are the
right trade-off; at the soak scale use `RELEASE=1` (launch 1) or the
release block (launch 2).
