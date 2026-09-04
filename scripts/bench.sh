#!/bin/sh
# The whole OEE bench in one command (week 5, D6 — also the week-6 demo):
#
#     scripts/bench.sh [scenario] [seed] [port]
#
# Starts the bench broker, generates the simulator streams for the scenario
# (default scenarios/week5/normal.toml, seed 42), and replays them through the
# three nodes while the ratatui dashboard is open in the foreground — the
# gauges update live during the replay and freeze on the final window.
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
AGGREGATOR=""
NODES=""

# One cleanup for the whole script: every background job that exists by
# the exit moment dies with it (empty PIDs are skipped).
cleanup() {
    for pid in "$BROKER" "$AGGREGATOR" "$NODES"; do
        if [ -n "$pid" ]; then
            kill "$pid" 2>/dev/null || true
        fi
    done
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

# The node replay as a FUNCTION, not an inline `( ... ) &` subshell: dash
# re-reads the script file around background subshells while the forked
# child shares the parent's read offset (the parent moving on to the next
# command corrupts what the child parses — observed as `--run-id: not
# found`); a parsed function body already lives in memory, so backgrounding
# the call is safe.
run_nodes() {
    # Give the foreground dashboard a second to connect and subscribe:
    # the broker has no retention, so only a subscription standing BEFORE
    # the first publish sees the stream.
    sleep 1
    ./target/debug/node --kind a --input "$OUT/run.csv" \
        --offline "$OUT/statuses.csv" --mqtt "$ADDR" --run-id "$RUN_ID" \
        >"$OUT/a.log" 2>&1
    ./target/debug/node --kind p --input "$OUT/ir.csv" \
        --offline "$OUT/counts.csv" --mqtt "$ADDR" --run-id "$RUN_ID" \
        >"$OUT/p.log" 2>&1
    ./target/debug/node --kind q --input "$OUT/taps.csv" --meta "$OUT/taps_meta.csv" \
        --offline "$OUT/verdicts.csv" --mqtt "$ADDR" --run-id "$RUN_ID" \
        >"$OUT/q.log" 2>&1
}
echo "== nodes (replay in the background — the dashboard below is live during it)"
run_nodes &
NODES=$!

# The dashboard goes up BEFORE/DURING the replay on purpose: the bench
# broker is QoS 0 with no retention, so a subscriber that connects after
# the publishes sees nothing ("waiting for data" forever). While the nodes
# stream (a few seconds), the gauges update live; after the end markers
# the values freeze on the final window — the state the demo walks through.
echo "== dashboard (live during the replay; q quits)"
./target/debug/oee-dashboard --mqtt "$ADDR" || true

echo "== waiting for the aggregator to flush on the end markers"
wait "$AGGREGATOR"
cat "$OUT/aggregator.log"
tail -n 2 "$OUT/a.log" "$OUT/p.log" "$OUT/q.log" 2>/dev/null

echo "== windows csv ($OUT/oee_windows.csv)"
cat "$OUT/oee_windows.csv"

echo "== done. Artifacts: $OUT"
