#!/bin/sh
# The whole OEE bench in one command (week 5, D6 — also the week-6 demo):
#
#     scripts/bench.sh [scenario] [seed] [port]
#
# Starts the bench broker, generates the simulator streams for the scenario
# (default scenarios/week5/normal.toml, seed 42), replays them through the
# three nodes, aggregates OEE = A x P x Q and opens the ratatui dashboard.
# Ctrl-C stops everything; the artifacts stay in tmp/bench/.
#
# The broker is the mqtt-min bench broker (no mosquitto needed). A real
# mosquitto works too: pass its port and start it yourself — every client
# speaks the same MQTT 3.1.1 subset.

set -eu

SCENARIO="${1:-scenarios/week5/normal.toml}"
SEED="${2:-42}"
PORT="${3:-18835}"
ADDR="127.0.0.1:$PORT"
OUT="tmp/bench"
RUN_ID="bench-$(basename "$SCENARIO" .toml)-$SEED"

echo "== building the bench (debug)"
cargo build -q -p mqtt-min -p line-simulator -p nodes -p oee-aggregator -p oee-dashboard

echo "== scenario $SCENARIO, seed $SEED, artifacts in $OUT"
mkdir -p "$OUT"

echo "== starting the bench broker on $ADDR"
./target/debug/broker "$PORT" >"$OUT/broker.log" 2>&1 &
BROKER=$!

cleanup() {
    kill "$BROKER" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

echo "== generating the simulator streams"
./target/debug/line-simulator --scenario "$SCENARIO" --seed "$SEED" \
    --out "$OUT/run.csv" \
    --taps-dataset "$OUT/taps.csv" --taps-meta "$OUT/taps_meta.csv" \
    --belt-events "$OUT/ir.csv" --belt-meta "$OUT/belt_meta.csv" 2>&1

echo "== aggregator (subscribes first — QoS 0 does not replay the past)"
./target/debug/aggregator --mqtt "$ADDR" --ideal-cycle-ms 400 \
    --out "$OUT/oee_windows.csv" >"$OUT/aggregator.log" 2>&1 &
AGGREGATOR=$!
sleep 0.5

echo "== nodes (replay the run through the model + MQTT)"
./target/debug/node --kind a --input "$OUT/run.csv" \
    --offline "$OUT/statuses.csv" --mqtt "$ADDR" --run-id "$RUN_ID" 2>&1
./target/debug/node --kind p --input "$OUT/ir.csv" \
    --offline "$OUT/counts.csv" --mqtt "$ADDR" --run-id "$RUN_ID" 2>&1
./target/debug/node --kind q --input "$OUT/taps.csv" --meta "$OUT/taps_meta.csv" \
    --offline "$OUT/verdicts.csv" --mqtt "$ADDR" --run-id "$RUN_ID" 2>&1

echo "== waiting for the aggregator to flush on the end markers"
wait "$AGGREGATOR"
cat "$OUT/aggregator.log"

echo "== windows csv ($OUT/oee_windows.csv)"
cat "$OUT/oee_windows.csv"

echo ""
echo "== dashboard: q quits (the bench data is already in MQTT history)"
./target/debug/oee-dashboard --mqtt "$ADDR" || true

echo "== done. Artifacts: $OUT"
