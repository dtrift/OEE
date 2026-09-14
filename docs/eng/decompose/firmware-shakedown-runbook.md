# Hardware track: the detailed shakedown runbook (updated 2026-09-14)

Russian original: [firmware-shakedown-runbook.md](../../rus/decompose/firmware-shakedown-runbook.md).

Context: the host track (weeks 1–6 + the report) is complete; 2026-09-14
closed S0 — all three boards are flashed and up (the release
`firmware/bin/20260914085743-*`, the logs are in the root READMEs, the
facts are in `firmware/NOTES.md`). The session plan —
`docs/eng/decompose/firmware.md`; the assembly guide —
`docs/eng/HARDWARE-assembly-guide.md`. Bridge serials: node A —
`5C94148486`, node Q — `5C94152266`, node P (CAM) — `5CCC048683`; there
are no `/dev/ttyUSB*` ports on this bench — the actual names are
`/dev/serial/by-id/usb-1a86_USB_Single_Serial_<SN>-if00`.

| Step | Topic                                       | Time   | Code needed?             |
| ---- | ------------------------------------------- | ------ | ------------------------ |
| 0    | the day's tails: commit the docs, CI        | 5 min  | —                        |
| 0.5  | the wire check of P                         | 5 min  | —                        |
| 1    | S1: node A, ADC and calibration             | ~3 h   | + the capture mode       |
| 2    | S2: node A, statuses across regimes         | ~2 h   | everything exists        |
| 3    | S3: the board vs the host twin cross-check  | ~2 h   | everything exists        |
| 4    | S4: node Q, the INMP441 I2S driver          | ~3 h   | + the I2S driver         |
| 5    | S5: node Q, the servo and the verdicts      | ~3 h   | everything exists        |
| 6    | S6: node P, part counting                   | ~2 h   | everything exists        |
| 7    | S7: the track gate and the phase-2 decision | ~2–3 h | + the gate docs          |
| 8    | phase 2: the bench in the full OEE loop     | ~1–2 h | exists (the uart-bridge) |

## Step 0 — the day's tails (5 min)

Commit the day's documentation (the root READMEs ×2, `firmware/README ×2`,
`firmware/NOTES.md`, `firmware/bin/20260914085743-build-info.md`) and
check CI on the `hardware-firmware` branch: the `firmware` job builds the
working tree with the `esp-bootloader-esp-idf` 0.6 dependency for the
first time (it requires rustc 1.95 or newer; CI installs a fresh espup,
expected green). If the job is red exactly on the rustc version — record
the fact in NOTES and roll the dependency back to 0.5 (minimum rustc
1.88).

## Step 0.5 — the wire check of node P (5 min)

Short GPIO5 to 3V3 with a jumper wire: hold for at least 60 ms, release,
repeat. The 50 ms debounce ignores closures shorter than 50 ms, so every
correct touch gives exactly one counter increment — the monitor shows a
`p,bench-p,<t_ms>,N` line with the next N. If the counter grows from
short touches (below 50 ms), the `DEBOUNCE_MS` constant in the firmware
does not match the actual parameter — that is a finding for NOTES, not
for a silent fix.

## Step 1 — S1. Node A: the ADC and calibration (~3 h)

The goal: prove with numbers that the current the board measures matches
the physical current. At rest the firmware must read zero (within the
tolerance); under a known load it must match the multimeter.

Wiring:

- ACS712-20A: VCC — 5 V, GND — common with the board, OUT — through a
  2:1 resistor divider (for example, 10 kΩ + 10 kΩ) to GPIO4
  (`board::node_a::ADC_CURRENT`). The divider is mandatory: the ACS712-20A
  output at zero current is 2.5 V, the sensitivity is 100 mV per ampere,
  the working range is 0.5–4.5 V; the ESP32-S3 ADC with 11 dB attenuation
  measures up to about 3.1 V, and the divider maps the sensor range into
  0.25–2.25 V.
- GPIO4 is chosen because it is an ADC1 channel: the ADC2 channels on the
  ESP32-S3 conflict with Wi-Fi, and phase 2 may theoretically use it.

What is already implemented: at startup the node averages 800 readings
taken every 625 μs (half a second, 1.6 kHz) and prints `a: zero=NNN` —
the startup zero point (`CurrentCalibration::acs712_20a_div2()` plus
`with_zero_counts`). Then every 128 readings (an 80 ms window) the whole
window goes into `model_a`; a status change is printed only after
confirmation by two consecutive windows (the ×2 hysteresis) as an
`a,bench-a,<t_ms>,<state>` line.

What is missing (this session's code): the capture mode. Today the board
prints only statuses, but the multimeter comparison and step 3 need
capture lines in the `features_cli::capture` schema —
`t_ms,node,run_id,value,state,note`. Add to `firmware-a` behind a
compile-time constant: once per window (80 ms) print a line with the
window-average current in amperes. Traffic estimate: 12.5 lines per
second at ~35 bytes — about 440 bytes per second against the UART's
115200 baud ≈ 11.5 kilobytes per second, a twentyfold margin. Printing
every individual sample is not possible: 1600 lines per second at the
same ~35 bytes give 56 kilobytes per second — the UART will not keep up
and lines will be lost.

Actions:

1. Wire the sensor as above; do not power the load yet.
2. Rebuild the image with the capture mode and flash board A (the port by
   the `5C94148486` serial), open the monitor recording to a file.
3. Rest: 60 seconds of recording. The criterion: the average `value` is
   at most 0.05 A in absolute value, the span (max minus min) is at most
   0.1 A. Tolerance derivation: one ADC count corresponds to about 15 mA
   (3.1 V / 4095 counts — that is 0.76 mV per count at the board pin, the
   divider doubles the voltage, and the sensor sensitivity is 100 mV per
   ampere); 0.05 A is three counts.
4. Linearity: the multimeter in the 10 A current mode in series with the
   load. Three levels: 0.5 A, 1.0 A, 2.0 A — resistors on 5 V: 10 Ω rated
   at least 5 W (0.5 A), 5.1 Ω at least 10 W (about 1.0 A), 2.7 Ω at
   least 25 W (about 1.9 A). Record 30 seconds at each level.
5. For each level compute the average `value` from the capture file and
   compare with the multimeter reading.

Pass criteria: rest — per item 3; linearity — at each level the
board-versus-multimeter deviation is at most the larger of 0.1 A and 5
percent of the reading; the `a: zero=` value changes by at most 5 counts
between two resets (Ctrl+R in the monitor).

Failure scenarios and their meaning: the readings are about half the
multimeter — the divider is not 2:1, or the sensor module has a different
sensitivity (the ACS712-5A is 185 mV per ampere; check the module marking
against the `acs712_20a_div2` constant); zero drifted while `zero=` is
stable — the calibration ran before the sensor was connected, reset the
board with the sensor attached; the values jump by ±0.5 A — no common
GND between the sensor supply and the board, or the input is floating.

Record in NOTES: the `zero` value with the sensor attached, the table
"level, board (A), multimeter (A), deviation", the final tolerance.

## Step 2 — S2. Node A: statuses across regimes (~2 h)

The goal: show that the status follows the physical regime within known
delay bounds and does not flip on its own inside a regime.

Scenario: the node distinguishes regimes by current; physically imitate
them with the same load from step 1 — alternate 0 A (30 seconds, the
idle regime) and 1–2 A (30 seconds, the run regime), at least five
switches each way and no more often than one switch per two seconds.

Where the delay bounds come from: a window of 128 readings at 1.6 kHz is
80 ms; the hysteresis confirms a change only after two consecutive
windows. So a status change cannot appear earlier than 80 ms and no later
than three windows, i.e. 240 ms.

Actions: record the monitor to a file for the whole run; for each switch
compute the delay — the status line time minus the actual switch time by
the stopwatch; count spontaneous flips inside the 30-second phases.

Pass criteria: each of the ten or more switches fits the 80–500 ms
interval (240 ms of theory plus a stopwatch-accuracy margin); spontaneous
flips inside the phases — zero over at least five minutes of recording.

Failure scenarios: the delay is consistently over a second — the actual
run current is below what the training set maps to the run regime
(compare the `value` during the phase with the week-4 experiment data);
the status flip-flops at the boundary — the phase current sits exactly at
the class-separation threshold, raise the run-current level (the
hysteresis must not be changed — the ×2 contract is pinned by tests).

Record in NOTES: the number of switches, the minimum, median and maximum
delays, the number of spontaneous flips.

## Step 3 — S3. The cross-check: the board vs the host twin (~2 h)

The goal: on one and the same physical run prove that the hardware node A
and the host node A produce the same status sequences, with discrepancies
allowed only at window boundaries.

Why boundary discrepancies are normal: both sides use the same model (the
rust-born `model_a` with the bit-exact Conv1D kernel), the same ×2
hysteresis, and the same input — the current in amperes computed on the
board. The only difference is the window phase: the board's windows start
from its boot moment, the host node's windows start from the first line
of the input file, so the samples at a regime-switch boundary fall into
different windows.

Actions:

1. Take the board capture file from step 1 or 2 (the monitor recording) —
   `tmp/s3.capture.csv`.
2. Convert it with the repository utility: it reads standard input,
   renames the `value` column to `current_a`, keeps the node A lines and
   writes the host node's ready input:
   `cargo run --release -p firmware-tools --bin capture-to-run < tmp/s3.capture.csv > tmp/s3.run.csv`
   (the utility prints a counter to stderr: how many lines were taken and
   how many skipped — the header, other nodes, bad lines).
3. Run the host node on the same run:
   `cargo run --release -p nodes --bin node -- --kind a --input tmp/s3.run.csv --offline tmp/s3.statuses.csv --run-id bench-a`
   (without `--mqtt` — an offline comparison, no broker needed).
4. Compare the two status sequences: the `a,bench-a,…` lines from the
   board's monitor recording against the lines from `tmp/s3.statuses.csv`.

Pass criteria: one hundred percent status agreement outside the zones
adjacent to regime switches (a zone is two windows, 160 ms, on each side
of a switch); inside the zones discrepancies are allowed. Any discrepancy
outside the zones is an escalation-level finding from the track plan
("why did the parity test not catch it"): fix the cause and close it with
a test, do not accept the discrepancy.

Record in NOTES: the table "total switches, agreed, discrepancies,
whether all discrepancies are inside boundary zones".

## Step 4 — S4. Node Q: the INMP441 I2S driver (~3 h, the only big piece of code)

The goal: replace the synthetic window with real sound. Today node Q's
verdict is always `cracked` — pinned by a test as the loop check; the
verdicts gain meaning only with the real microphone.

INMP441 wiring: VDD — strictly 3.3 V (at 5 V the module dies), GND —
common, the L/R channel-select input — to GND (the left channel), SCK —
GPIO12 (`board::node_q::I2S_SCK`), WS — GPIO13 (`I2S_WS`), SD — GPIO14
(`I2S_SD`).

Code:

1. Add an I2S receiver to `firmware-q`: esp-hal, the `i2s::master` module
   (the driver modules are already exposed by the `unstable` feature),
   the Philips standard mode, the 16 kHz sample rate (the `window_spec(Q)`
   contract), a 32-bit slot, one channel (left), reception over DMA into
   a 1024-slot buffer (4 kilobytes). The window is read with one blocking
   read right after the servo strike.
2. The slot conversion is already written and pinned by host tests:
   `i2s_slots_to_f32` takes the top 24 bits of the 32-bit slot,
   sign-extends and divides by 2 to the power 23 — the full scale comes
   out at about ±1.0. The byte and channel order is exactly the question
   the conversion was lifted into a testable library for.
3. In the main loop replace the `synthetic_tap_window(window)` call with
   the DMA window read and conversion. Do not delete `synthetic_tap_window`
   from the library: its verdict is pinned by a test and stays the
   reference point.
4. A temporary dump mode (behind the same compile constant as the capture
   in step 1): print the first window after startup — the spectrum is
   computed from it.

The tone check:

1. Play a 1 kHz tone near the microphone (a generator or a phone tone
   app, normal speech loudness).
2. Take a window dump and compute the spectrum on the host by any means
   (for example numpy: the discrete Fourier transform of the 1024
   values).
3. Expectation: the peak in bin 64. The math: the bin width is 16000 Hz
   / 1024 = 15.625 Hz; 1000 Hz / 15.625 = bin 64; the ±5 percent
   tolerance — the 950–1050 Hz band, bins 61–67.
4. The sample rate is confirmed by the same peak: were it half, the 1 kHz
   tone would land in bin 32. The window duration is 1024 / 16000 =
   exactly 64 ms.
5. Amplitude: in silence the values are around 0.001–0.01, on the tone —
   0.1–0.5.

Failure scenarios: exact zeros in every window — the L/R channel-select
level is wrong, or SCK and WS are swapped; a raw-slot dump (before
conversion) will show zeros immediately. The peak in bin 32 — the actual
sample rate is 8 kHz, an I2S configuration error. Noise across all bins
instead of a peak — pickup on the data line: shorten the wires, check the
common GND.

Record in NOTES: the discovered byte and channel order (whether it
matched the conversion contract), the peak bin, the silence and tone
amplitudes.

After this step — re-release the images into `firmware/bin/` by the same
procedure as 2026-09-14 (a new timestamp, hashes, the old set to
`tmp/trash/`), because the Q firmware behavior changes.

## Step 5 — S5. Node Q: the servo tapper and the verdicts on reference parts (~3 h)

The goal: the verdicts depend on the part. A good part yields mostly
`good`, a cracked one — mostly `cracked`.

Servo wiring: the SG90 signal wire to GPIO11
(`board::node_q::SERVO_PWM`); the servo power — only from a separate 5 V
supply rated at least 1 A, the supply's minus tied to the board's GND; a
470 μF capacitor across the servo supply terminals. The failure scenario:
if the servo is powered from the board's USB port, at the strike moment
the servo draws an inrush of about 0.5–1 A, the 5 V rail sags below the
processor's brownout threshold, and the board reboots mid-tap — in the
monitor it looks like a sudden repeat of the boot banner.

What is already implemented: a 50 Hz PWM with 14-bit duty; the strike —
6 percent duty (a ~1.2 ms pulse), rest — 8 percent (~1.6 ms); after the
strike a 30 ms pause (the arm leaves, the part's ring begins), then the
64 ms window; the full cycle is 400 ms. Where the arm strikes and its
length are mechanical-assembly parameters, not code.

Actions:

1. Mount the servo over the part holder so that the arm touches the part
   on actuation — the strike must be audible.
2. Power the servo from the separate supply and make sure the board does
   not reboot across ten consecutive strikes (no repeated boot banners in
   the monitor).
3. Five good parts: at least three strikes each, write down all the
   verdicts.
4. Five cracked parts: the same.
5. If the dump mode from step 4 is still in the firmware — capture one
   window for a good and a cracked part: the spectra must differ
   noticeably.

Pass criteria: for each part at least two thirds of the strikes give its
class verdict; in total at least eight of ten parts are classified
correctly. Every discrepancy — into NOTES with a description of the part.

Failure scenarios: the verdicts do not depend on the part (for example
all `cracked`, as with the synthetic) — the window missed the ring; a
dump will show the sound starting after the window start, and then the
post-strike pause is increased in the firmware (the `SETTLE_MS` constant,
currently 30 ms, the next sensible step is 50 ms), or the servo
physically misses the part. The verdicts are right but the board reboots
on strikes — the supply is still insufficient: at the strike moment the
servo terminals must stay above roughly 4.5 V by multimeter.

Record in NOTES: the servo strike current (a multimeter in the supply
break), the per-part verdict table, any `SETTLE_MS` changes.

## Step 6 — S6. Node P: part counting (~2 h)

The goal: the counter equals the fact — N passed parts give exactly N
increments, and the bounce at the sensor edge creates no false counts.

The mandatory pre-mount check: node P lives on the CAM board, the sensor
is assigned to GPIO5 (`board::node_p::IR_OUT`), but the OV2640 camera
wiring on boards from different vendors occupies different pins, and on
some boards GPIO5 belongs to the camera. Check your board's pinout
against its schematic: if GPIO5 is taken, the sensor moves to a free pin
and the fix lands in the `board` crate (the bench-configuration level;
the node contract does not change). The `used_pins_are_free_and_distinct`
test will verify that the new pin is not in the reserved list: 0, 3, 45,
46 — the boot-strapping pins; 19, 20 — the USB lines; 43, 44 — the UART0
console; 35, 36, 37 — the octal-PSRAM lines of the N16R8 module.

TCRT5000 wiring: the module with a comparator — VCC per the module's
marking (3.3 or 5 V), GND common, OUT to GPIO5. The sensor is reflective
infrared: it triggers on reflection from an object at roughly 1–25 mm,
the threshold is tuned by the on-module trimmer.

Actions:

1. Tune the distance: a part at 5–10 mm from the sensor — the module's
   LED lights up.
2. The quick wire check from step 0.5 (every correct touch longer than
   60 ms — exactly one increment).
3. Run 1: 20 slow passes of a part (or a finger) — the final count
   exactly 20.
4. Run 2: 20 passes at the pace of the future belt.
5. The bounce test: hold a target at the triggering edge and wiggle it
   for five seconds.

Pass criteria: in both runs the count-versus-fact deviation is zero; the
bounce test gives at most two extra increments in five seconds (the 50 ms
debounce suppresses series shorter than 50 ms; sustained re-crossings
longer than 50 ms are real events already).

Failure scenarios: the count is double the fact — the signal drops below
the threshold and returns mid-pass (visible on an OUT-level recording):
increase the sensor gap or reduce the sensitivity with the trimmer. The
count is zero although the module's LED triggers — the module has an
inverted output (active low): check the OUT level at rest and at
triggering with a tester; on inversion the fix goes into the pin-reading
driver in the firmware (not the contract) and is recorded in NOTES.

Record in NOTES: the trigger polarity, the distance, the results of both
runs, the bounce-test result.

## Step 7 — S7. The track gate and the phase-2 decision (~2–3 h)

The goal: shape the S0–S6 results into verifiable artifacts and make the
phase-2 decision.

Actions:

1. Finalize `firmware/NOTES.md`: each session must have its numbers — S1:
   `zero`, the linearity table, the tolerance; S2: the status delays; S3:
   the cross-check table; S4: the I2S byte and channel order, the tone
   bin; S5: the servo current, the per-part verdicts; S6: the sensor
   polarity and distance, the counting results.
2. Write `docs/rus/firmware-gate.md` and `docs/eng/firmware-gate.md`
   following the weekly-gate pattern (week1–week6-gate). The track gate
   checklist is listed in the decomposition: the A statuses match the
   host ones on the same data; the Q verdicts on reference parts match
   the model on the same windows; the P count equals the run's fact; the
   board's capture CSV is readable by the host tooling and reproducible;
   the CI `firmware` job is green; NOTES is filled. For every item —
   passed, or a deviation with the reason. The "blinky skipped, the
   working firmware-a flashed at once" deviation is already in NOTES —
   carry it into the gate too.
3. The phase-2 decision with justification: the UART bridge (already
   written and tested, not a single line of network code on the boards)
   versus esp-wifi with a ported mqtt-min (a week of extra work plus the
   risk of the bench's Wi-Fi environment). Recommendation: the bridge —
   it closes the goal today; Wi-Fi — into the report's future work.
4. Check that both CI jobs (`firmware` — the xtensa build with the new
   dependency, `firmware-host` — formatting, clippy, the library tests)
   are green on the latest commit.

Pass criterion: both gate docs are written, every checklist item has a
status, the phase-2 decision is recorded with its justification.

## Step 8 — phase 2: the bench in the full OEE loop (~1–2 h, after the gate)

The goal: the physical bench replaces the host nodes in the week-5 loop —
the three boards' lines reach MQTT through the UART bridge, the
aggregator folds them into OEE, the dashboard shows live metrics.

Actions:

1. Terminal 1 — the loop's tail end (the "The bench loop" section of
   `firmware/README.md`):
   `./target/debug/broker 1883 &`;
   `./target/debug/aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 --out windows.csv`;
   `./target/debug/oee-dashboard --mqtt 127.0.0.1:1883`.
2. Terminals 2–4 — one bridge per board, the ports by the serials:
   `cat /dev/serial/by-id/usb-1a86_USB_Single_Serial_5C94148486-if00 | cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883`
   and two analogous commands for the serials `5C94152266` (Q) and
   `5CCC048683` (P). The bridge drops the boot lines (pinned by its
   tests), publishes the statuses, verdicts and counts to `oee/line1/*`,
   and on input-stream break prints the end marker — the aggregator
   closes the window correctly.
3. The no-hardware check (known): feed the bridge four lines via printf —
   expect `bridge: 4 messages (a+p+q)` in the output.
4. The live run: turn on the physical stimuli — the current for node A,
   the taps for Q, the part passes for P — and watch the dashboard.

Pass criterion: over a five-minute run the dashboard shows A status
changes, Q verdicts with every tap, the P count equal to the passed
parts, and `windows.csv` fills with window rows without gaps between the
end markers.

Failure scenarios: the dashboard is empty, the bridge is silent — the
board line does not pass the bridge's whitelist; compare the actual line
from the monitor with the printf-test line format (`a,bench-a,1000,run`).
The OEE does not converge with the expectation — the 400 ms ideal-cycle
parameter is given to the aggregator while the bench's actual tap pace
differs; reconcile `--ideal-cycle-ms` with the fact (it is an aggregator
parameter, the firmware does not change).

A documentation note: the assembly guide
(`docs/eng/HARDWARE-assembly-guide.md` and its Russian original) names
the ports with the old `/dev/ttyUSB0` — on this bench the actual names
are `/dev/ttyACM*` or by-id; update the guide when convenient.
