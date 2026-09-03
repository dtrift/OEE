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
- [ ] The divider on the ACS712's OUT is assembled (2×10 kΩ), the ADC input
      is GPIO4.
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
