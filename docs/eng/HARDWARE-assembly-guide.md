# OEE Bench — Hardware Assembly & Firmware Guide

> Generated: 2026-09-12
> Status: reference for first physical bring-up
> Prerequisites: code-only track complete (weeks 1–6), hardware purchased

## Contents

1. [Bill of Materials](#1-bill-of-materials)
2. [Board Pin Reference](#2-board-pin-reference)
3. [Toolchain Setup](#3-toolchain-setup)
4. [Building Firmware](#4-building-firmware)
5. [Wiring Guide](#5-wiring-guide)
6. [Assembly Order](#6-assembly-order)
7. [Pre-Power Checklist](#7-pre-power-checklist)
8. [Flashing & First Power-On](#8-flashing--first-power-on)
9. [Bring-Up Sessions](#9-bring-up-sessions)
10. [Phase 2: UART Bridge to MQTT](#10-phase-2-uart-bridge-to-mqtt)
11. [Safety](#11-safety)

---

## 1. Bill of Materials

### Boards (already purchased 2026-08-20)

| Qty | Board                               | Role                 | Notes                               |
| --- | ----------------------------------- | -------------------- | ----------------------------------- |
| 1   | ESP32-S3-DevKitC-1 N16R8            | Node A (current)     | ADC1 + WiFi                         |
| 1   | ESP32-S3-DevKitC-1 N16R8            | Node Q (audio)       | I2S + servo PWM                     |
| 1   | ESP32-S3-WROOM-1 N16R8 CAM + OV2640 | Node P (IR counting) | Camera pins taken — check schematic |

### Sensors & Actuators

| Component                    | Node | Purpose                               |
| ---------------------------- | ---- | ------------------------------------- |
| ACS712-20A + 2×10 kΩ divider | A    | Machine current measurement           |
| INA226 (optional)            | A    | I2C current sensor, no divider needed |
| INMP441 I2S microphone       | Q    | Acoustic tap detection                |
| SG90 servo + 10–15 cm stick  | Q    | Tapper actuator                       |
| TCRT5000 × 2                 | P    | Part counting + belt-end sensor       |

### Mechanics & Misc

| Component                                  | Notes                                           |
| ------------------------------------------ | ----------------------------------------------- |
| 100×350 mm conveyor (0–116 rpm)            | With speed regulator                            |
| Drill or fan + M5 bolt                     | The "machine" — imbalance load                  |
| M5–M8 nuts/washers (~50 pcs)               | Parts riding the belt                           |
| "Rejects" — sawn/chipped parts (10–15 pcs) | Marked with marker for node Q ground truth      |
| MB-102 breadboard (830 points) × 1–4       | One per node is cleaner                         |
| M-M / M-F jumper wires                     | White = signals, yellow = I2S, blue = servo PWM |
| 470 µF electrolytic capacitor              | At servo power pins                             |
| 5 V 2 A power supply (separate)            | Servo only — never share 5 V rail with boards   |
| USB-C data cables × 3                      | Must be data cables, not charge-only            |

### What to Check When Ordering

- Module marking must read **N16R8** (16 MB flash + 8 MB octal PSRAM). Clones with N8R2 (2 MB PSRAM) are too small for the camera and spare role.
- Two USB-C ports on the board — native OTG and UART bridge.
- Clones from AliExpress work fine; reference is `ESP32-S3-DevKitC-1` silkscreen.

---

## 2. Board Pin Reference

**Single source of truth**: `firmware/board/src/lib.rs`. Wiring changes land here, not in node firmwares.

### Node A — Current (ACS712)

| Signal                         | GPIO | ADC      | Notes                                |
| ------------------------------ | ---- | -------- | ------------------------------------ |
| ACS712 OUT (after 2:1 divider) | 4    | ADC1_CH3 | ADC1 only — ADC2 conflicts with WiFi |

### Node Q — Audio + Servo

| Signal         | GPIO | Notes       |
| -------------- | ---- | ----------- |
| I2S SCK (BCLK) | 12   | INMP441     |
| I2S WS (LRCL)  | 13   | INMP441     |
| I2S SD         | 14   | INMP441     |
| Servo PWM      | 11   | 50 Hz, LEDC |

### Node P — IR Counting (CAM board)

| Signal       | GPIO | Notes                                    |
| ------------ | ---- | ---------------------------------------- |
| TCRT5000 OUT | 5    | CAM board — verify free of camera wiring |

### Reserved Pins (do NOT use)

```
GPIO 0, 3, 45, 46   — strapping
GPIO 19, 20         — USB D−/D+
GPIO 35, 36, 37     — octal PSRAM on N16R8
GPIO 43, 44         — UART0 / console
```

### Wire Color Discipline

| Color  | Use                      |
| ------ | ------------------------ |
| Red    | +5 V (servo supply only) |
| Orange | +3.3 V                   |
| Black  | GND                      |
| White  | Signals (ADC, IR, PWM)   |
| Yellow | I2S lines                |
| Blue   | Servo signal             |

**Critical**: Never tie the two 5 V sources' pluses together. Only grounds meet.

---

## 3. Toolchain Setup

The firmware uses a separate Rust workspace with the Xtensa toolchain. It does not affect the host CI.

### Install espup

```bash
cargo install espup
espup install
```

### Source the environment (each new shell)

```bash
. $HOME/export-esp.sh
```

### Add the rust-src component (required for build-std)

```bash
rustup component add rust-src --toolchain esp
```

### Verify

```bash
rustup show
# Should list xtensa-esp32s3-none-elf as available
```

---

## 4. Building Firmware

### Host Build (tests, no toolchain needed)

```bash
cd firmware
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

### Target Build (Xtensa)

```bash
cd firmware
export PATH="$HOME/.rustup/toolchains/esp/bin:$PATH"
cargo build --release \
    -p firmware-a -p firmware-q -p firmware-p \
    --target xtensa-esp32s3-none-elf
```

Outputs:
- `target/xtensa-esp32s3-none-elf/release/firmware-a` → Node A
- `target/xtensa-esp32s3-none-elf/release/firmware-q` → Node Q
- `target/xtensa-esp32s3-none-elf/release/firmware-p` → Node P

### Install espflash

```bash
cargo install espflash
```

---

## 5. Wiring Guide

### 5.1 Node A — Current Sensor

The ACS712-20A outputs 2.5 V at zero current. The ESP32-S3 GPIOs are NOT 5 V-tolerant, so a 2:1 voltage divider is mandatory.

**ACS712 module wiring:**

| ACS712 Pin | Connection                                  |
| ---------- | ------------------------------------------- |
| VCC        | Board 5 V pin (red jumper to bottom + rail) |
| GND        | Board GND (black jumper to − rail)          |
| OUT        | → 10 kΩ → GPIO4 → 10 kΩ → GND               |

**Breadboard layout (rows 20–22):**

```
Row 20: ACS712 OUT --[10 kΩ]-- Row 21
Row 21: --[10 kΩ]-- Row 22 → bridge to (−) rail
Row 21 (tap point): → jumper (white) → Board GPIO4
```

**Multimeter check before USB power-up:** between GPIO4 jumper and GND, expect ~1.2–1.3 V with the machine off (ACS712 zero point).

**ACS712 in the machine circuit:** break the machine's low-voltage power line (≤36 V only!), insert ACS712 in series: `PSU 12 V → IP+ ... IP- → fan`. **Do NOT wire into 220 V mains.**

### 5.2 Node Q — Microphone + Servo

**INMP441 wiring:**

| INMP441 Pin | Connection                          |
| ----------- | ----------------------------------- |
| VDD         | 3.3 V (orange jumper to top + rail) |
| GND         | GND (black jumper to − rail)        |
| SCK (BCLK)  | GPIO12 (yellow jumper)              |
| WS (LRCL)   | GPIO13 (yellow jumper)              |
| SD          | GPIO14 (yellow jumper)              |
| L/R         | GND (bridge to adjacent GND row)    |

Keep I2S wires ≤15 cm to reduce noise.

**Servo wiring (separate 5 V supply — NOT board USB):**

| SG90 Pin        | Connection                                       |
| --------------- | ------------------------------------------------ |
| Signal (orange) | GPIO11 (blue jumper)                             |
| Plus (red)      | Separate 5 V PSU +                               |
| Minus (brown)   | Separate 5 V PSU − AND board GND (common ground) |

**470 µF capacitor:** between servo +5 V and GND at the breadboard. Long leg (plus) to row 8, short leg (marked stripe) to row 10.

### 5.3 Node P — IR Barrier (CAM board)

**TCRT5000 module wiring:**

| TCRT5000 Pin | Connection                          |
| ------------ | ----------------------------------- |
| VCC          | 3.3 V (orange jumper to top + rail) |
| GND          | GND (black jumper to − rail)        |
| OUT          | GPIO5 (white jumper)                |

**Trim procedure:**
1. Empty belt, powered: turn trimmer until "detection OFF" LED is solid.
2. Pass a nut across the gap: "detection" LED blinks per pass.
3. If LED constantly on: increase distance or shield from reflections.
4. If LED never reacts: decrease distance.

If the module is inverted (OUT high when empty), fix in `firmware/p/src/main.rs` — not in the counter logic.

---

## 6. Assembly Order

Follow this order with checks at each step:

### Step 1: Mechanics
- Assemble conveyor per kit instructions.
- Mount the "machine" (drill/fan with M5 bolt) at the belt's start.
- Mark the tap position at 2/3 of the belt length.
- **Check:** belt runs from its adapter; adapter voltage matches motor marking.

### Step 2: Empty Breadboard
- Connect top (−) rail to bottom (−) rail with a jumper.
- **Check:** continuity beep top(−) ↔ bottom(−) ≈ 0 Ω.

### Step 3: Node A
- Assemble the ACS712 divider on rows 20–22.
- Connect jumpers to the board's GPIO4 and power rails.
- **Check:** USB only, machine off → `a: zero=NNN` in UART monitor.

### Step 4: Node Q
- Place INMP441 across the center groove (rows 1–5).
- Wire servo header (rows 8–12) with separate PSU and 470 µF capacitor.
- **Check:** 50 Hz square wave on GPIO11 (logic analyzer if available).

### Step 5: Node P
- Place TCRT5000 module on rows 30–32.
- Trim the potentiometer by the LED indication.
- **Check:** stable OUT with empty belt, edge per part.

### Step 6: Common Ground (final touch)
- Tie servo PSU's minus to node Q board's GND.
- **Check:** all grounds connected (boards, servo PSU, machine, conveyor).

---

## 7. Pre-Power Checklist

- [ ] USB-C cables are data cables (verify by attempting to flash)
- [ ] Servo powered from separate supply, minus tied to board GND
- [ ] 470 µF capacitor in place at servo header, polarity correct
- [ ] ACS712 divider assembled (2×10 kΩ, rows 20–22), tap into GPIO4
- [ ] Multimeter on GPIO4 tap ≈ 1.2–1.3 V with machine off
- [ ] TCRT5000 trimmer tuned (empty belt = quiet, part = blink)
- [ ] Current sensor in machine's low-voltage circuit only (≤36 V)
- [ ] INMP441 L/R on GND, I2S jumpers tight
- [ ] CAM board: node P pins free of camera wiring
- [ ] Common ground: servo PSU, all boards, machine, conveyor all tied on minus

---

## 8. Flashing & First Power-On

### Identify USB Ports

```bash
ls /dev/ttyUSB*
# Expect: /dev/ttyUSB0, /dev/ttyUSB1, /dev/ttyUSB2
```

### Flash Each Board

```bash
# Node A
espflash flash --port /dev/ttyUSB0 \
    target/xtensa-esp32s3-none-elf/release/firmware-a

# Node Q
espflash flash --port /dev/ttyUSB1 \
    target/xtensa-esp32s3-none-elf/release/firmware-q

# Node P
espflash flash --port /dev/ttyUSB2 \
    target/xtensa-esp32s3-none-elf/release/firmware-p
```

If a board is not seen, hold **BOOT** button while plugging in.

### Monitor UART Output

```bash
# Using espflash
espflash monitor --port /dev/ttyUSB0

# Or with picocom
picocom /dev/ttyUSB0 -b 115200

# Or without installing anything
stty -F /dev/ttyUSB0 115200 raw -echo && cat /dev/ttyUSB0
```

### Expected Output

**Node A:**
```
a: boot, run_id=bench-a
a: zero=1638    # varies with hardware
a,bench-a,8000,run
```

**Node Q:**
```
q: boot, run_id=bench-q
q,bench-q,30,good     # every ~400 ms (synthetic window until S4)
```

**Node P:**
```
p: boot, run_id=bench-p
p,bench-p,50,1        # per part passing the gap
```

### Permission Issues

```bash
sudo usermod -aG dialout $USER
# Re-login required
```

---

## 9. Bring-Up Sessions

Follow the staged bring-up plan (`docs/eng/decompose/firmware.md`):

### S0 — Toolchain + Blinky (~2 h)
- Verify espup toolchain works
- Flash a blinky to confirm board is alive
- Run `espflash flash-info` to verify flash size (16 MB) and PSRAM

### S1 — Node A: ADC + Calibration (~3 h)
- Verify ADC1 samples at ~1.6 kHz
- Confirm startup zero calibration works
- Check capture CSV format over UART

### S2 — Node A: Window → Predict → Status (~3 h)
- Wire up the fork (`../fork/microflow` + nalgebra patch)
- Verify model classification on real current data
- Confirm hysteresis prevents rapid state flips

### S3 — Cross-Check: Board vs Host (~2 h)
- Convert board capture CSV to host node input
- Run host node A on the same data
- Compare statuses — mismatches only at window boundaries

### S4 — Node Q: I2S Microphone (~3 h)
- Verify INMP441 samples at 16 kHz
- Dump a 1024-sample window over UART
- Confirm tone lands in expected spectrum bin

### S5 — Node Q: Servo + Verdict (~3 h)
- Verify servo strikes at correct timing
- Test model classification on real tap audio
- Confirm verdicts on reference parts (good/cracked)

### S6 — Node P: Part Counting (~2 h)
- Verify TCRT5000 edge detection
- Confirm 50 ms debounce eliminates bounces
- Run N parts, verify count = N

### S7 — Gate + NOTES (~2–3 h)
- Document shakedown facts in `firmware/NOTES.md`
- Write firmware gate document
- Decide on phase 2 (UART bridge vs WiFi)

---

## 10. Phase 2: UART Bridge to MQTT

Once all three nodes produce status lines, connect the bench to the full OEE loop:

### Start the Broker + Aggregator + Dashboard

```bash
# Terminal 1
./target/debug/broker 1883 &
./target/debug/aggregator --mqtt 127.0.0.1:1883 --ideal-cycle-ms 400 --out windows.csv
./target/debug/oee-dashboard --mqtt 127.0.0.1:1883
```

### Run the UART Bridge

```bash
# Terminal 2 — one board at a time, or all three if using separate ports
cat /dev/ttyUSB0 | cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```

The bridge reads status lines from stdin and publishes to `oee/line1/*` on MQTT. It emits `{node}/end` markers when the stream closes, which signals the aggregator to finalize.

### Test Without Hardware

```bash
printf 'a: boot, run_id=bench-a\na,bench-a,1000,run\nq,bench-q,2000,good\np,bench-p,3000,1\n' | \
    cargo run -p firmware-tools --bin uart-bridge -- 127.0.0.1:1883
```

Expected: `bridge: 6 messages (a+p+q)` and dashboard gauges update.

---

## 11. Safety

- **Voltage limits:** All sensors and wiring must be in circuits ≤36 V. The 220 V mains is out of reach for breadboard wiring.
- **Clamp the drill:** Secure the "machine" to prevent movement during operation.
- **Fan preference:** A fan is safer than a drill for the demo.
- **Servo clearance:** Keep fingers out of the tap position while the servo is active. The stick is not metal.
- **Power down before rewiring:** Unplug USB and servo supply before any wiring changes.
- **ACS712 placement:** Only in the machine's low-voltage power line, never in 220 V.

---

## Troubleshooting

| Symptom                             | Likely Cause                       | Fix                                                                  |
| ----------------------------------- | ---------------------------------- | -------------------------------------------------------------------- |
| Garbage on UART                     | Wrong baud or wrong USB port       | Use 115200 baud, UART port (not OTG)                                 |
| Board not flashing                  | In download mode                   | Hold BOOT while plugging in                                          |
| Servo jitters or reboots board      | Missing common ground or capacitor | Tie servo PSU GND to board GND, add 470 µF                           |
| Node A readings drift               | ACS712 zero drift                  | Firmware recalibrates at startup — ensure machine is off during boot |
| Node P counts bounces               | Debounce too short                 | Increase `DEBOUNCE_MS` in `firmware-p`                               |
| Node Q verdicts wrong               | I2S wiring or phase                | Dump raw window, check byte order against reference                  |
| Model diverges from host            | Quantization difference            | Check `#[model]` path, compare int8 ranges                           |
| "Permission denied" on /dev/ttyUSB* | User not in dialout group          | `sudo usermod -aG dialout $USER`, relogin                            |

---

## Key Files Reference

| File                                    | Purpose                               |
| --------------------------------------- | ------------------------------------- |
| `firmware/board/src/lib.rs`             | Single source of truth for bench pins |
| `firmware/a/src/main.rs`                | Node A firmware entry                 |
| `firmware/q/src/main.rs`                | Node Q firmware entry                 |
| `firmware/p/src/main.rs`                | Node P firmware entry                 |
| `firmware/tools/src/bin/uart-bridge.rs` | UART → MQTT bridge                    |
| `features-cli/src/calibration.rs`       | ADC → amps conversion contract        |
| `features-cli/src/lib.rs`               | Window specs per node                 |
| `docs/eng/bench-assembly.md`            | Detailed wiring diagrams              |
| `docs/eng/decompose/firmware.md`        | Bring-up session plan                 |
| `docs/eng/equipment.md`                 | Purchase list and board notes         |
