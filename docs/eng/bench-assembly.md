# Test Bench Assembly and Wiring (the hardware track)

> Purpose: a step-by-step guide for assembling the bench and wiring its
> components — from boxes of boards to a shakedown-ready rig. The single
> source of truth for pins is the [`board`](../../firmware/board/src/lib.rs)
> crate (wiring changes land only there; this document follows it). The
> purchase list — [`equipment.md`](./equipment.md) (short) and the detailed
> one in `kontext/`; the firmware bring-up order —
> [`decompose/firmware.md`](./decompose/firmware.md).
>
> The bench: 2× ESP32-S3-DevKitC-1 (N16R8) — nodes A and Q; 1×
> ESP32-S3-WROOM-1 N16R8 CAM with OV2640 — node P + the stretch camera.
> Assemble one node per session (1 breadboard in stock out of the 4 planned)
> — the order: A → Q → P.

## 1. The bench bill of materials

| Block       | Components                                                                                                      |
| ----------- | --------------------------------------------------------------------------------------------------------------- |
| Boards      | 2× DevKitC-1 N16R8, 1× WROOM-1 N16R8 CAM (OV2640 on board)                                                      |
| Sensors     | ACS712-20A (+ a 2×10 kΩ divider), an INA226 option, INMP441, TCRT5000 ×2                                        |
| Actuator    | SG90 + a wooden stick tapper (~10–15 cm)                                                                        |
| Mechanics   | a 100×350 mm conveyor (0–116 rpm, check the adapter's voltage), the "machine": a drill or a fan                 |
| The parts   | M5–M8 nuts/washers (~50 pcs); "rejects" — sawn/chipped, 10–15 pcs, marked with a marker (node Q's ground truth) |
| Consumables | an 830-hole breadboard, M-M/M-F jumpers, resistors, a 470 µF capacitor, a 5 V 2 A supply, USB-C data cables     |

## 2. The bench layout (mechanics)

1. The conveyor is the base: the parts ride the belt from the "machine"
   towards the end.
2. The "machine" (a drill/fan with a bolt on a blade for imbalance) — at the
   belt's start; powered from its own supply (≤36 V), with node A's current
   sensor inserted into that power circuit. **Do not wire the sensors into
   the 220 V mains.**
3. Node A (DevKitC-1 #1) — at the machine: measures the drive current.
4. Node Q's tap position — on the part's path: the servo taps the part with
   the stick, the microphone 2–5 cm from the strike point.
5. Node P (the CAM board) — at the belt's end: TCRT5000 #1 counts the parts
   coming off, #2 (optional) — the "belt end" sensor.

Top view (the scale is nominal):

```text
                ┌──[machine PSU 12 V]─╪═ IP+ ACS712 IP− ═╪─┐
                │      (a break in the machine's power line)   │
                ▼                                              ▼
        ╔══════════╗    ┌─[DevKitC-1 #1 + ACS712]            ╔═╗
        ║ MACHINE  ║    │   (node A: the drive current)      ║P║ ── [DevKitC-1 #2]
        ║ fan/drill ║    ▼                                    ║a║     (node Q: servo +
        ║ + a bolt  ║ ────▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶▶─── ║r║      INMP441)
        ╚══════════╝         THE CONVEYOR BELT             ╚═╝      ▲ the tap position:
                       nuts/washers ride the belt →→→→→→→→→ └── [the CAM board]
                                                        the end:   (node P: the TCRT5000
                                                        TCRT5000   in the drop-off gap)
```

The mechanical assembly order:

1. The conveyor — per its kit's instructions (motor + gearbox + the speed
   regulator); leave the regulator at minimum before the first power-on.
2. The machine — at the belt's start, clamped; the "load" is an M5 bolt on
   the fan's blade.
3. The tap position — 2/3 of the belt's length from the machine: a part
   stops at a stick stopper (a skewer across the belt), the servo with its
   stick is above the position, the INMP441 is nearby (2–5 cm, the
   membrane towards the strike point).
4. Node P's gap — at the belt's drop-off: the parts fall/pass through the
   TCRT5000 beam; the beam height — at the center of a passing nut.

## 3. Power and the common ground

- Each board — through its own USB-C data cable (not charge-only!) into the
  host: flashing, power, and the UART log over one cable.
- The SG90 servo — **strictly from a separate 5 V 2 A supply**, not the
  board's USB: the servo's inrush current sags the rail and reboots the
  board. The 470 µF capacitor — at the servo's power pins (mind the
  polarity).
- **The common ground**: the servo supply's minus ties to the node Q board's
  GND (otherwise the PWM signal has no reference — the servo won't run or
  will jitter).
- The conveyor — from its bundled adapter; check the adapter's output
  voltage against the motor's marking before powering on.

## 3a. The breadboard plan

**The boards do NOT go onto the breadboard.** A DevKitC-1 plugged into a
breadboard occupies both middle banks (a–e and f–j) for its whole length —
no room for the modules. So: the boards lie next to the breadboard (on
silicone feet), the modules and passives sit on the breadboard, the
connections are M-F jumpers from the board's pins into the breadboard
rows.

An MB-102 breadboard (830 points): two power rail pairs along the edges
(`+`/`−`) and 63 rows × 2 middle banks. While there is a single
breadboard, all three nodes live on it (the layout below); when the rest
arrive — one breadboard per node, each block moves over as-is.

**The single-breadboard layout across three nodes** (the row numbers are
nominal — the point is not to overlap):

```text
 top (+) rail:  3.3 V (from the Q board's 3V3 pin)     ←— low-current only!
 top (−) rail:  GND (common to all boards)

 rows 1–5:   [NODE Q]  INMP441 (the module across the center groove)
 rows 8–12:  [NODE Q]  the servo header: PSU +5 V, GND, the GPIO11 signal;
                       the 470 µF capacitor between +5 V and GND HERE
 rows 20–24: [NODE A]  the ACS712 divider: OUT—10k—●—10k—GND,
                       the tap ● — to GPIO4 (the diagram in section 4a)
 rows 30–32: [NODE P]  TCRT5000: VCC/OUT/GND (section 6a)

 bottom (+) rail: 5 V (from the A board's 5V pin) — the ACS712's VCC only
 bottom (−) rail: a GND continuation (a jumper from the top (−))
```

**Wire color discipline** (it saves debugging time): red — +5 V, orange —
+3.3 V, black — GND, white — signals (ADC/IR/PWM), yellow — the I2S lines,
blue — the servo signal. The servo PSU's plus and minus — separate wires
(red/black), never through the shared rails.

**Never tie the two 5 V sources' pluses together**: the board's USB 5 V
and the servo PSU's 5 V are separate branches; ONLY the grounds meet.

## 3b. The assembly order (step by step, with checks)

1. **Mechanics** (section 2): the conveyor assembled, the machine at the
   belt's start, the tap position marked. Check: the belt runs from its
   adapter; the adapter's voltage matches the motor's marking.
2. **An empty breadboard**: a jumper between the top and bottom (−) rails;
   nothing else yet. Check: a continuity beep top-(−)↔bottom-(−) ≈ 0 Ω.
3. **Node A** (sections 4, 4a): the divider — jumpers — the ACS712 into
   the machine's line. Check: USB only, the machine off → `a: zero=NNN`
   in the UART.
4. **Node Q** (sections 5, 5a): the INMP441 — jumpers; the servo — the PSU
   + the capacitor. Check: a 50 Hz square wave on GPIO11 (the logic
   analyzer, if at hand).
5. **Node P** (sections 6, 6a): the TCRT5000, the trimmer — by the
   module's indicator LED. Check: a stable OUT with an empty belt, an
   edge per part.
6. **The common ground** — the final touch: the servo PSU's minus ↔ the Q
   board's GND.

## 4. Node A — current (DevKitC-1 #1)

Wiring the ACS712-20A through a divider (the S3 pins are not 5 V-tolerant):

| ACS712 module pin | To                                               |
| ----------------- | ------------------------------------------------ |
| VCC               | 5 V (the board's 5V pin)                         |
| GND               | the board's GND                                  |
| OUT               | → the divider: OUT —10 kΩ— **GPIO4** —10 kΩ— GND |

- The sensor breaks into the machine's power circuit **in series**
  ("supply → sensor → machine"), the low-voltage side only, ≤36 V.
- GPIO4 is ADC1: for node A use ADC1 pins only (ADC2 conflicts with WiFi);
  the ADC1 range is GPIO1–GPIO10.
- The ACS712's zero drifts — the firmware does a startup recalibration
  (`CurrentCalibration::with_zero_counts`): at startup the machine is off.
- The INA226 option (I2C, no divider): assign SDA/SCL when choosing this
  branch and pin them as constants in `board` (not wired yet).

### 4a. Node A's breadboard diagram

```text
                the ACS712-20A module (lies next to the breadboard:
   ┌───────────┐ the IP+/IP− screw terminals are wider than the grid)
   │ VCC GND OUT│        │
   └──┼───┼──┼─┘        │  IP+ ═══ IP− — in the BREAK of the machine's
      │   │  │                  power line (PSU 12 V → IP+ ... IP− → fan)
      │   │  └──── jumper (white) → row 20
      │   └─────── jumper (black) → the (−) rail
      └────────── jumper (red) → the bottom (+) rail = 5 V from the A board's 5V pin

 The divider in rows 20–22 (the resistors "across" the center groove):

 row 20: ACS712 OUT ──[10 kΩ]── row 21
 row 21: ──[10 kΩ]── row 22 → row 22 bridged into the (−) rail
                              │
 row 21 (the tap ●) ── jumper (white) → the A board's GPIO4
```

Why exactly this: the top leg's drop at zero current is 2.5 V → GPIO4 sees
1.25 V (ADC 11 dB, the range up to ~3.1 V); at +20 A the input is 2.25 V —
headroom on both sides. Rows 20–22 keep the divider as a self-contained
block — easier to probe with a multimeter.

A multimeter check before the USB power-up: between the GPIO4 jumper and
(−) — about 1.2–1.3 V with the machine off (that is the ACS712's "zero").

## 5. Node Q — sound and the tapper (DevKitC-1 #2)

INMP441 (the S3's I2S matrix — any GPIO):

| INMP441 module pin | To                                                    |
| ------------------ | ----------------------------------------------------- |
| VDD                | 3.3 V                                                 |
| GND                | GND                                                   |
| SCK (BCLK)         | GPIO12                                                |
| WS (LRCL)          | GPIO13                                                |
| SD (data)          | GPIO14                                                |
| L/R                | GND (the left channel; check the module's silkscreen) |

The SG90 servo:

| SG90 pin | To                                                                |
| -------- | ----------------------------------------------------------------- |
| Signal   | GPIO11 (50 Hz PWM)                                                |
| Plus     | the separate 5 V supply (+470 µF at the pins)                     |
| Minus    | the supply **and** the board's GND (the common ground, section 3) |

The tapper stick: a 10–15 cm skewer (30 cm springs); to the servo horn —
hot glue or a zip tie; the strike — a short stroke onto the part at the tap
position.

### 5a. Node Q's breadboard diagram

The INMP441 module's pins are 2.54 mm pitch — it plugs into the breadboard
across the center groove (pins into rows 1–5, bank f–j):

```text
 INMP441 (a view of the module's pins; the order — per YOUR module's marking!)
 ┌─────────────────────┐
 │ VDD GND SD SCK WS L/R│
 └─┼───┼──┼───┼───┼──┼─┘
   │   │  │   │   │  └─ a bridge to the adjacent GND row (the left channel)
   │   │  │   │   └── jumper (yellow) → GPIO13 (WS)
   │   │  │   └────── jumper (yellow) → GPIO12 (SCK)
   │   │  └────────── jumper (yellow) → GPIO14 (SD)
   │   └───────────── a bridge into the (−) rail
   └───────────────── jumper (orange) into the top (+) rail = the Q board's 3.3 V
```

Keep the I2S wires short (≤15 cm): 16 kHz tolerates longer ones, but
keeping the noise out of the audio path from the start is cheaper; the
microphone — on a hot-glued stand 2–5 cm from the tap position, the
membrane towards the strike point.

The servo is NOT on the breadboard: its three wires arrive at rows 8–12 as
the "servo power header":

```text
 row  8:  [servo PSU +5 V] ──●─ ──► the SG90's red wire
               │            │
               │        470 µF (the minus leg into row 10!)
               │            │
 row 10:  [GND] ─────────●─ ──► the SG90's brown wire
                        │
                        └── a bridge into the (−) rail → the Q board's GND
 row 12:  [signal] ←── a jumper (blue) from GPIO11 ──► the SG90's orange wire
```

The capacitor is an electrolytic: the long leg (plus) into row 8, the
short leg/marked stripe (minus) into row 10. Plug the servo in only after
a polarity check with a multimeter (a steady 5 V between rows 8 and 10).

## 6. Node P — counting (the WROOM-1 N16R8 CAM board)

TCRT5000 ×2 (the module already has a comparator, no resistors needed):

| TCRT5000 module pin | To                                                                                                |
| ------------------- | ------------------------------------------------------------------------------------------------- |
| VCC                 | 3.3 V (per the module's marking)                                                                  |
| GND                 | GND                                                                                               |
| OUT (counting)      | GPIO5 — the DevKitC-layout assignment; on the CAM board take a pin free of the camera (see below) |
| OUT #2 (optional)   | the "belt end"; a second free pin + a constant in `board`                                         |

- The CAM board carries the OV2640: the camera wiring takes some GPIOs —
  before assigning node P's pins, cross-check the specific board's
  schematic (CAM boards from different sellers differ in the layout); pin
  the chosen pins as constants in `board` and here, in section 7.
- The camera itself connects nowhere (a stretch, shaked down separately).

### 6a. Node P's breadboard diagram and the threshold trim

The TCRT5000 module (3–4 pins: VCC, GND, OUT, some also + A0) — into rows
30–32 across the groove; only the digital OUT is used:

```text
 TCRT5000
 ┌─────────────┐
 │ VCC GND OUT (A0 unused)
 └─┼───┼───┼─┘
   │   │   └── jumper (white) → the CAM board's GPIO5
   │   └────── a bridge into the (−) rail
   └────────── jumper (orange) into the top (+) rail = the P board's 3.3 V
```

Mounting on the belt: the emitter/receiver look ACROSS the direction of
travel — into the gap between the belt and the drop-off (or through a slot
in a guide rail); 2–10 mm from a passing part; the mount — a skewer
bracket/a small clamp, so a knock does not shift it.

The trimmer procedure (the module has a multi-turn pot + two LEDs):

1. The belt empty, powered: turn until the "detection OFF" LED is solid
   (OUT in the "empty" state — low level in our wiring; the LED marking
   per the module's silkscreen).
2. Pass a nut across the gap: the "detection" LED blinks per pass.
3. If the LED is constantly on (it sees the belt/guide) — increase the
   distance or shield the emitter's side window from reflections with
   cardboard; if it never reacts — decrease the distance/press closer.
4. The firmware counts a RISING edge (a part entered the gap): if the
   module turns out inverted (OUT high when empty) — invert it in
   `firmware-p/src/main.rs` (the spot is marked with a comment), not in
   the counter.

## 7. The consolidated pin table (a mirror of `board`)

| Node | Signal        | GPIO | Note                                         |
| ---- | ------------- | ---- | -------------------------------------------- |
| A    | ACS712 (ADC1) | 4    | after the 2:1 divider; ADC1 only             |
| Q    | I2S SCK       | 12   | INMP441                                      |
| Q    | I2S WS        | 13   | INMP441                                      |
| Q    | I2S SD        | 14   | INMP441                                      |
| Q    | Servo PWM     | 11   | power from a separate supply + common ground |
| P    | IR OUT        | 5    | on the CAM board — a pin free of the camera  |

Taken by the board/chip (do not use): GPIO0/3/45/46 (strapping), GPIO19–20
(USB D−/D+), GPIO35–37 (the octal PSRAM on the N16R8), GPIO43–44
(UART0/console).

## 8. The pre-power checklist

- [ ] The USB-C cables are data (verify by flashing: a charge-only won't
      flash).
- [ ] The servo: powered from the separate supply, its minus tied to the
      board's GND, the 470 µF in place, the polarity correct.
- [ ] The divider on the ACS712's OUT is assembled (2×10 kΩ, rows 20–22),
      the tap into GPIO4; a multimeter on the tap ≈ 1.2–1.3 V with the
      machine off.
- [ ] The 470 µF capacitor — in the servo header's rows 8/10, the long
      leg (plus) in row 8; a steady 5 V from the separate PSU between rows
      8 and 10.
- [ ] The TCRT5000 trimmer tuned by the LEDs (an empty belt — quiet, a
      part — a blink); an inverted level — fix in `firmware-p/src/main.rs`.
- [ ] The current sensor is in the machine's low-voltage circuit (≤36 V),
      not in 220 V.
- [ ] The INMP441's L/R is on GND; the I2S lines are short, the jumpers sit
      tight.
- [ ] On the CAM board, node P's chosen pins do not collide with the camera
      or the taken list (section 7).
- [ ] The common ground: the servo supply, the boards, the machine, the
      conveyor — all tied on the minus.

## 9. First power-on (smoke)

Per the shakedown decomposition sessions
([`decompose/firmware.md`](./decompose/firmware.md)):

1. **S0**: the espup toolchain, a blinky on every board; `esptool.py
   flash_id` — show the real flash size (16 MB) and the PSRAM (to rule out
   an N8R2 swap).
2. **S1**: node A's ADC — with the machine off, ~0 A (the zero
   recalibration); a multimeter vs the readings at 2–3 currents.
3. **S4**: node Q's I2S — a tone of a known frequency at the microphone
   lands in the expected spectrum bin (the window exactly 64 ms).
4. **S5**: the servo — a single tapper stroke (the machine and the parts in
   place, the separate supply); the acoustic verdict on reference parts.
5. **S6**: node P — a run of N parts, the count = N, bouncing gives no
   repeats.

## 10. Safety

- The sensors and the INA226 — only into circuits ≤36 V (the machine/
  conveyor supplies). The 220 V mains is out of reach for breadboard wiring.
- The drill is clamped; a fan is safer for the demo.
- Keep fingers out of the tap position with the servo active; the stick is
  not metal.
- Before any rewiring — unplug the USB and the servo supply.
