# Node P: the build and bring-up plan for real hardware

Russian original: [firmware-p-bringup.md](../../rus/decompose/firmware-p-bringup.md).

> Date: 2026-09-16, updated 2026-09-17 (the physical-testing fact sheet is
> at the end). The methodology follows node A in the `firmware-a` branch
> (check-based diagnostics, a symptom cheat sheet, a facts journal). Sources:
> [`firmware/README.md`](../../../firmware/README.md) (build/flash),
> [`docs/eng/bench-assembly.md`](../bench-assembly.md) (wiring, sections
> 3a/6/6a), [`firmware-shakedown-runbook.md`](./firmware-shakedown-runbook.md)
> (steps 0.5 and 6), [`firmware/NOTES.md`](../../../firmware/NOTES.md) (the journal).

## Starting point (bench facts)

| What        | State                                                                                                                                                                                                           |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Board       | ESP32-S3-WROOM-1 N16R8 CAM (OV2640 onboard), a CH343 bridge, serial `5CCC048683`                                                                                                                                |
| Port        | `/dev/serial/by-id/usb-1a86_USB_Single_Serial_5CCC048683-if00` (confirmed 2026-09-17; the short `USB_Serial` form from the Russian runbook header does not exist on this bench)                                 |
| S0          | Closed 2026-09-14: the `firmware/bin/20260914085743-firmware-p` image flashed (app 99,040 bytes — the smallest of the three nodes), the boot line received; the silence afterwards is the norm without a sensor |
| Peculiarity | P is an event-driven node: no periodicity, a line only per part (A/Q emit a line every cycle)                                                                                                                   |
| Branch      | the 2026-09-17 test ran on `firmware-p`; the stage-6 aggregator comes from `firmware-a` (the 15 s ping fix)                                                                                                     |

## Stage map

| Stage | Topic                             | Time    | Code?                           |
| ----- | --------------------------------- | ------- | ------------------------------- |
| 0     | tooling (the toolchain)           | ~30 min | —                               |
| 1     | host-side logic checks            | ~10 min | —                               |
| 2     | building the release image        | ~10 min | —                               |
| 3     | mounting + tuning the TCRT5000    | ~1 h    | —                               |
| 4     | flashing and the wire-check smoke | ~20 min | —                               |
| 5     | S6: counting parts on the belt    | ~2 h    | everything exists (point fixes) |
| 6     | wiring into the OEE loop          | ~30 min | everything exists (uart-bridge) |

## Stage 0 — tooling (once, ~30 min on a clean machine)

The same as for nodes A/Q (the bench's shared toolchain):

```bash
cargo install espup espflash
espup install                    # the patched Xtensa toolchain → ~/.rustup/toolchains/esp
. $HOME/export-esp.sh            # the linker PATH (xtensa-esp-elf-gcc)
asdf reshim rust                 # with asdf rust: the binaries land in ~/.asdf/.../bin
sudo usermod -aG dialout $USER   # port rights; in the current shell — newgrp dialout
```

The NOTES gotcha: the esp toolchain lives in `~/.rustup/toolchains/esp`, which
the asdf-shell rustup does not see — cargo picks the shell's stable and fails
on the `-Z` flags. Before every target build:

```bash
export PATH="$HOME/.rustup/toolchains/esp/bin:$PATH"
```

Check: `espflash --version` ≥ 4.6 (older ones refuse to flash an image
without the ESP-IDF app descriptor, which the firmware already embeds).

## Stage 1 — host-side logic checks (~10 min, no hardware)

```bash
cd firmware && cargo test --workspace
```

What is pinned by the tests and matters for P: the `EdgeCounter` logic
(the 50 ms debounce — `DEBOUNCE_MS`, the rising edge, a glitch storm must
not freeze the counter) and the `format_count` line format
(`p,bench-p,777,131\n` — a test in the lib). Red tests = stop: there is no
point going further.

## Stage 2 — building the release image (~10 min)

```bash
cd firmware
export PATH="$HOME/.rustup/toolchains/esp/bin:$PATH"
. $HOME/export-esp.sh
cargo build --release -p firmware-p --target xtensa-esp32s3-none-elf
```

The artifact: `firmware/target/xtensa-esp32s3-none-elf/release/firmware-p`
(a single ELF; espflash turns it into a bootable image itself — no separate
.bin needed). Release, not debug: panics print to the UART anyway. Do NOT
flash `qemu/target/.../oee-qemu` — a different target (LM3S6965/Cortex-M3).

## Stage 3 — mounting the TCRT5000 and setting the threshold (~1 h)

Per [`docs/eng/bench-assembly.md`](../bench-assembly.md): section 3a (the
shared plan), 6 (the pinout), 6a (the row layout and tuning). The board does
NOT go into the breadboard (it would eat both center rails) — it lies next
to it; the module sits on the breadboard, connections with M-F jumpers.

**The mandatory pre-mount check** (the runbook, step 6): node P lives on the
CAM board, the sensor is assigned to GPIO5 (`board::node_p::IR_OUT`), but the
OV2640 camera wiring occupies different pins on boards from different
vendors, and on some boards GPIO5 belongs to the camera. Cross-check your
board's pinout against its schematic. If GPIO5 is taken, the sensor moves to
a free pin and the fix lands in the `board` crate (the bench configuration
level; the node contract does not change). The `used_pins_are_free_and_distinct`
test will check the new pin against the reserved list: 0, 3, 45, 46
(boot-strapping pins), 19, 20 (the USB lines), 43, 44 (the UART0 console),
35, 36, 37 (the octal PSRAM lines of the N16R8 module).

The TCRT5000 module (3–4 pins: VCC, GND, OUT, some add A0) — into rows 30–32
across the groove; only the digital OUT is used:

| Pin | Where                                                       |
| --- | ----------------------------------------------------------- |
| VCC | 3.3 V (per the module's marking; some modules are 5 V)      |
| GND | the (−) rail                                                |
| OUT | GPIO5 of the CAM board (or the chosen free pin — see above) |
| A0  | not used                                                    |

The sensor is reflective infrared: it triggers on reflection at ~1–25 mm;
the threshold is trimmed by the multi-turn pot on the module.

Belt mounting: the emitter/receiver look ACROSS the travel — into the gap
between the belt and the exit (or through a slot in the guide); 2–10 mm to
the passing part; the bracket is a skewer/clamp so a knock does not shift it.

Pot tuning (a multi-turn resistor + two LEDs on the module):

1. Empty belt, powered: turn until the "detection OFF" LED is stable (OUT in
   the "empty" state — low for our wiring).
2. Carry a nut past the gap: the "detection" LED flashes on every pass.
3. The LED is constantly on (it sees the belt/guide) — increase the distance
   or shield the emitter's side window with cardboard against re-reflections.
4. No reaction — decrease the distance / press closer.

Check before connecting to the board: with a meter, the module's OUT level
at rest and on detection (bring a finger/nut) — write the polarity down. The
firmware counts the RISING edge (a part entered the gap; the input pull is
down); if the module inverts (OUT high at idle), the fix goes into
`firmware-p/src/main.rs` (the place is marked with a comment), not into the
counter.

## Stage 4 — flashing and the wire-check smoke test (~20 min)

From the `firmware/` directory (the artifact path is `target/...`; from the
repo root — `firmware/target/...`):

```bash
espflash flash -M --port /dev/serial/by-id/usb-1a86_USB_Single_Serial_5CCC048683-if00 \
    target/xtensa-esp32s3-none-elf/release/firmware-p
# -M = flash and drop into the monitor at once; a separate monitor (if the
# board is already flashed):
# espflash monitor --port /dev/serial/by-id/usb-1a86_USB_Single_Serial_5CCC048683-if00
```

The NOTES gotcha (caught on this board 2026-09-14): flash and monitor only
through the bridge «COM» port (by-id `usb-1a86_*`). The CAM board's native
«USB» port (Espressif `303a:4001`) gives «Failed to connect» — the automatic
download-mode entry does not fire. If even the bridge does not see the board
— hold BOOT while plugging in.

Expected in the monitor (115200):

```text
p: boot, run_id=bench-p
...silence is the norm: P has no periodicity, only events
```

The wire-check reaction test (the runbook, step 0.5): short GPIO5 to 3V3
with a jumper — hold at least 60 ms, release, repeat. The 50 ms debounce
ignores closures shorter than 50 ms, so every correct touch gives exactly
one increment:

```text
p,bench-p,1200,1
p,bench-p,3100,2
...
```

If the counter grows from short touches (below 50 ms), the `DEBOUNCE_MS`
constant does not match the actual parameter — a finding for NOTES, not a
silent fix.

## Stage 5 — S6: counting parts on the belt (~2 h)

Goal: the counter equals the fact — N passed parts give exactly N
increments; bounce at the sensor edge creates no false ones. Details — the
runbook, step 6.

Actions:

1. Trim the distance: a part at 5–10 mm from the sensor — the module LED
   lights up.
2. Repeat the quick wire check from stage 4 (every touch longer than 60 ms —
   exactly one increment).
3. Run 1: 20 slow passes of a part (or a finger) — the final count is
   exactly 20.
4. Run 2: 20 passes at the future belt pace.
5. The bounce test: hold the target at the detection edge and wiggle it for
   five seconds.

Pass criteria: in both runs the count-to-fact discrepancy is zero; the
bounce test gives at most two extra increments per five seconds (the 50 ms
debounce suppresses series shorter than 50 ms; sustained threshold
re-crossings longer than 50 ms are real events already).

Failure scenarios: the count is double the fact — the signal crosses the
threshold and returns during a pass (visible on an OUT-level recording):
increase the sensor gap or reduce the sensitivity with the pot. The count is
zero although the module LED fires — the module's output is inverted (active
low): check the OUT level at rest and on detection with a meter; on
inversion the fix goes into the pin-read driver in the firmware (not the
contract) and is recorded in NOTES.

Record into `firmware/NOTES.md`: the trigger polarity, the distance, both
run totals, the bounce-test result.

If the pin (GPIO5 taken by the camera) or the level inversion changed along
the way — re-release the images into `firmware/bin/` per the 2026-09-14
procedure: a new timestamp, sha256, build-info.md; the old set goes to
`tmp/trash/`.

## Stage 6 — wiring into the OEE loop (~30 min, after the gate)

The aggregator — from the `firmware-a` branch (the fix lives there: a ping
every 15 s, strictly inside the broker's 30 s idle cut; it is not in `main`
yet). Commands — from the repo root:

```bash
./target/debug/broker 1883 &
./target/debug/aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 --out windows.csv --expect a,p
./target/debug/oee-dashboard --mqtt 127.0.0.1:1883
```

A separate terminal — the board's lines into MQTT:

```bash
cat /dev/serial/by-id/usb-1a86_USB_Single_Serial_5CCC048683-if00 | \
    cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```

The P gauge on the dashboard comes alive on every part; the bridge filters
the boot line (pinned by its tests). A live bridge check without the board:

```bash
printf 'p: boot, run_id=bench-p\np,bench-p,3000,1\n' | \
    cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```

## The "it does not work — why" cheat sheet (an addendum to bench-assembly section 9a)

| Symptom                                  | Cause                                             | Fix                                                                           |
| ---------------------------------------- | ------------------------------------------------- | ----------------------------------------------------------------------------- |
| garbage in UART                          | wrong baud / wrong port                           | 115200, the bridge «COM» port (by-id `usb-1a86_*`)                            |
| the board won't flash: Failed to connect | the board's native «USB» port                     | replug into «COM»; if the bridge — hold BOOT while plugging                   |
| No such file or directory on flash       | a wrong by-id name form                           | the long form `usb-1a86_USB_Single_Serial_<SN>-if00`; `ls /dev/serial/by-id/` |
| boot present, then silence               | the norm without a sensor (P is event-driven)     | the GPIO5 → 3V3 wire check                                                    |
| the wire does not increment the count    | a touch shorter than 50 ms (debounce)             | hold ≥60 ms, release, repeat                                                  |
| the count grows on its own               | the sensor threshold on the edge / OUT noise      | the 30 s "empty" check; LED sync; pull OUT — silence                          |
| the count grows from short touches       | `DEBOUNCE_MS` does not match the fact             | a finding for NOTES, not a silent fix                                         |
| the module LED is constantly on          | it sees the belt/guide (re-reflections)           | increase the distance, cardboard on the emitter side window                   |
| the LED flashes, the count is zero       | OUT inverted (active low)                         | meter the level; fix in `firmware-p/src/main.rs`, not the counter             |
| the count is double the fact             | the signal jitters across the threshold on a pass | increase the gap / reduce the pot sensitivity                                 |
| the part is not seen at all              | too far / the gap misses the part center          | 5–10 mm, the beam at the passing nut's center                                 |
| empty gauges with a live P               | the aggregator with the default `--expect a,p,q`  | restart with `--expect a,p`                                                   |

## Node P readiness criteria (the local S6 gate)

- [ ] GPIO5 cross-checked against the CAM board schematic (free of the
      camera), or the pin moved with the `used_pins_are_free_and_distinct`
      test green.
- [ ] Wire check: every ≥60 ms touch — exactly one increment; <50 ms touches
      grow nothing.
- [ ] Run 1: 20 slow passes — the count is exactly 20.
- [ ] Run 2: 20 passes at the belt pace — the count is exactly 20.
- [ ] The bounce test: at most two extra increments per 5 s.
- [ ] The OUT polarity and the working distance recorded in NOTES.
- [ ] If the pin or the inversion was fixed — the image re-released into
      `firmware/bin/` (timestamp, sha256, build-info), the old set into
      `tmp/trash/`.

## Safety

- Before any re-wiring — unplug the board's USB.
- The sensor is low-power (3.3/5 V per the module marking); mains 220 V is
  out of reach for breadboard work.
- P has no actuators — it is the simplest of the three nodes: no servo-power
  or tap-position risk here.

## The physical-testing fact sheet (2026-09-17)

### Flashing and bring-up — passed

- The `firmware-p` branch, a fresh release ELF (227,584 bytes, built 09:38);
  the app that landed in flash is 99,040 bytes — the same figure as the
  `20260914085743-firmware-p` release.
- The board confirmed its passport: esp32s3 rev v0.2, a 40 MHz crystal,
  16 MB flash (matches NOTES 2026-09-14).
- The convenient launch form: `espflash flash -M ...` — flash and monitor
  in one command; CTRL+C frees the port for the uart-bridge.

### The port-name gotcha (closed 2026-09-17)

Symptom: `Error while connecting to device — No such file or directory` at
the connect stage (before the ELF is read). Cause: the short form
`usb-1a86_USB_Serial_<SN>-if00` (the Russian runbook header) does not exist
on this bench; the observed name is built from the bridge's product string
`USB Single Serial`:

```text
/dev/serial/by-id/usb-1a86_USB_Single_Serial_5CCC048683-if00
```

Diagnostics (the step order): `lsusb` → the bridge `1a86:55d3 QinHeng "USB
Single Serial"` is visible; `/sys/class/tty/ttyACM0` exists (the kernel
created the port); the product string → the by-id name. The correct form is
recorded in `firmware/README.ru.md` (commit 5d9d370); the mismatch with the
Russian runbook is a candidate for a repo-doc fix.

A side gotcha: the first attempt ran from the `firmware/` directory with the
`firmware/target/...` path (a doubled `firmware/`) — from `firmware/` the
path is `target/...`, from the repo root `firmware/target/...`; the port
error masked it for a while.

The espflash WARN "Monitor options were provided..." is harmless: monitor
options from the user config are ignored without the `-M` flag.

### The monitor output (the boot log trimmed, the count lines verbatim)

```text
rst:0x1 (POWERON),boot:0x8 (SPI_FAST_FLASH_BOOT)
I (29) boot: ESP-IDF v6.1-beta1-... 2nd stage bootloader
I (30) boot: chip revision: v0.2
I (45) boot: Partition Table: nvs / phy_init / factory
I (133) boot: Loaded app from partition at offset 0x10000
p: boot, run_id=bench-p
p,bench-p,5456,1
p,bench-p,8242,2
p,bench-p,45593,3
p,bench-p,51157,4
p,bench-p,53379,5
p,bench-p,54488,6
p,bench-p,56502,7
p,bench-p,59269,8
p,bench-p,60093,9
p,bench-p,64269,10
p,bench-p,65606,11
p,bench-p,66800,12
p,bench-p,74173,13
p,bench-p,77278,14
p,bench-p,96869,15
p,bench-p,97838,16
```

### Interpreting the count lines

Each line is one accepted rising edge on GPIO5 (a high level ≥50 ms,
debounced), `t_ms` since boot; 16 events over ~98 s. The fork:

- the cadence (a pair of touches → a ~37 s pause → a burst) looks like the
  manual GPIO5 → 3V3 wire check — then stage 4 is closed: exactly +1 per
  touch, the bounce is eaten, the line format is correct;
- if there were no touches — spontaneous edges. The 1–3 s intervals rule
  out mains hum (50 Hz would give tens of events per second); the likely
  source is the TCRT5000 threshold (a hand/part/IR background at the
  detection edge).

The "empty" check (mandatory before S6):

1. Remove everything from the sensor's field of view and stay still for
   30 s — the count must freeze.
2. The module LED flashes in sync with the lines → the sensor really fires
   (pot tuning, stage 3); the LED is silent while the lines come → noise on
   the OUT wire: shorten it, check the ground reference.
3. The control experiment: pull the OUT wire off GPIO5 — the internal
   pull-down must give complete silence.

### Stage status

| Stage                             | Status as of 2026-09-17                               |
| --------------------------------- | ----------------------------------------------------- |
| 0–2: toolchain, host tests, build | the build passed (the ELF at 09:38); logic per branch |
| 3: mounting + threshold tuning    | pending — the LED tuning has not happened yet         |
| 4: flashing + smoke               | passed (with the caveat about the edge source)        |
| 5: S6 — counting on the belt      | the next step                                         |
| 6: the OEE loop                   | after the gate                                        |
