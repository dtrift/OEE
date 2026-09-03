# Detailed Purchase List: the OEE Bench (the Hardware Variant)

> Context: the approved plan is code-only ([`plan.md`](plan.md)); this list is
> for a live defense demo and/or hardware future work. Short version:
> `kontext/equipment.md` (working notes). Prices are a 2026 AliExpress
> ballpark; Ozon/WB run 30–50% higher but deliver in 1–2 days. Spares policy:
> cheap and fragile components ×2; identical boards are interchangeable.

## 1. The totals at a glance

| Block                 | Mandatory (order on day 0) | With options (deferred order) |
| --------------------- | -------------------------- | ----------------------------- |
| Boards (3 nodes)      | ~$30                       | ~$30                          |
| Sensors and the servo | ~$14                       | ~$16                          |
| Bench (the belt)      | ~$0–30                     | ~$0–30                        |
| Consumables           | ~$25                       | ~$25                          |
| Options               | —                          | ~$17                          |
| **Total**             | **~$70–100**               | **~$85–120**                  |

Purchase status (2026-08-20, cart closed): the boards are **DevKitC-1 ×2 +
one ESP32-S3 WROOM N16R8 CAM board with OV2640** (item 18alt, which also
closes item 18) — the composition is in sections 2 and 11; the sums above
are the historical estimate as of planning time.

"Mandatory" includes the spares per the policy above (a second
ACS712/INMP441/SG90, a third TCRT5000 — all cheap, cheaper than a second
delivery). The bench's "~$0–30": the belt is either a DIY kit (~$30) or
3D-print/LEGO + a motor from stock ($0). The "machine" (a drill or a fan) —
from around the house, $0.

## 2. Boards — the backbone of the bench

| №     | Component                             | Qty | Specs                                                 | ~$/ea | Total | Priority           |
| ----- | ------------------------------------- | --- | ----------------------------------------------------- | ----- | ----- | ------------------ |
| 1     | ESP32-S3-DevKitC-1 **N16R8**          | 2   | 16 MB flash, 8 MB octal PSRAM, 2×USB-C                | 10    | 20    | mandatory          |
| 18alt | ESP32-S3 WROOM N16R8 **CAM** + OV2640 | 1   | the same N16R8 + a camera socket and a bundled OV2640 | 15    | 15    | mandatory (bought) |

Roles: node A (current) and node Q (microphone + servo) — on the DevKitC-1;
node P (IR counting) — on the CAM board, which also carries the stretch
camera (the OV2640 is already on board — a separate item 18 is not needed).
All three boards are ESP32-S3 N16R8: any node's firmware builds for any
board, one toolchain; interchangeability is limited only by the pins the
camera takes on the CAM board. When ordering, check the module marking:
`ESP32-S3-WROOM-1-N16R8` (sellers often slip in an N8R2 — 2 MB quad PSRAM).

## 3. Node A — current (Availability)

| № | Component           | Qty | Specs                        | ~$/ea | Total | Priority              |
| - | ------------------- | --- | ---------------------------- | ----- | ----- | --------------------- |
| 2 | ACS712-20A (module) | 2   | analog output, 5 V           | 2     | 4     | 1 main + 1 spare      |
| 3 | INA226 (module)     | 1   | I2C, 3.3 V, no divider       | 2     | 2     | option (more precise) |
| 4 | 10 kΩ resistors     | 10  | a 2:1 divider for the ACS712 | —     | —     | from kit №11          |

Wiring: the ACS712 output through a divider (two 10 kΩ resistors) → an
**ADC1** pin (GPIO4…GPIO10; on the S3, ADC1 = GPIO1–GPIO10; do not use ADC2 —
it conflicts with WiFi). The divider-free, solder-free alternative is the
INA226 over I2C, but its sample rate is lower (~1 kHz is enough for the
current envelope; for the waveform only the ACS712 will do).

## 4. Node Q — sound (Quality)

| № | Component                | Qty | Specs                  | ~$/ea | Total | Priority         |
| - | ------------------------ | --- | ---------------------- | ----- | ----- | ---------------- |
| 5 | INMP441 (I2S microphone) | 2   | 24-bit, 3.3 V, I2S     | 1.5   | 3     | 1 main + 1 spare |
| 6 | SG90 (servo)             | 2   | 9 g·cm, 5 V, 50 Hz PWM | 2     | 4     | 1 main + 1 spare |
| 7 | 470 µF capacitor         | 3   | electrolytic, ≥6.3 V   | 0.3   | 1     | mandatory        |
| 8 | Wooden stick             | 5   | a skewer / sushi stick | —     | —     | the tapper       |

Wiring: on the S3, I2S goes to any GPIO (the matrix): SCK→GPIO12, WS→GPIO13,
SD→GPIO14; the servo PWM→GPIO11. The servo power is a **separate 5 V**
(supply №17), not the board's USB: the servo's inrush current sags the rail
and reboots the board; capacitor №7 — at the servo's power pins.

## 5. Node P — IR counting (Performance)

| №  | Component          | Qty | Specs                   | ~$/ea | Total | Priority                      |
| -- | ------------------ | --- | ----------------------- | ----- | ----- | ----------------------------- |
| 9  | TCRT5000 (module)  | 3   | IR barrier, digital out | 0.5   | 1.5   | counting + "belt end" + spare |
| 10 | Nuts/washers M5–M8 | ~50 | 10–15 pieces, some sawn | —     | —     | the parts; mark the rejects   |

Wiring: the module's OUT → GPIO5 (the module already has a comparator, no
resistors needed). Counting — on the edge with software debouncing (a ~50 ms
window). Node P lives on the CAM board (item 18alt): pick the IR pin free of
the camera and of the section 9 list — cross-check the exact camera pinout
of your board against its schematic at bring-up.

## 6. The bench: the machine and the belt

| №  | Component                   | Qty | Specs                            | ~$ | Priority                      |
| -- | --------------------------- | --- | -------------------------------- | -- | ----------------------------- |
| 11 | DIY conveyor-belt kit       | 1   | ~50–60 cm, motor + speed control | 30 | or 3D-print/LEGO from stock   |
| 12 | "Machine": a drill or a fan | 1   | whatever is at home              | 0  | imbalance — a bolt on a blade |

The parts are nuts/washers; "rejects" are made by sawing or chipping,
10–15 pieces, mark them with a marker (this is the ground truth for node Q).
A fan is safer than a drill for the demo.

## 7. Consumables

| №  | Component                            | Qty    | ~$ | Note                           |
| -- | ------------------------------------ | ------ | -- | ------------------------------ |
| 13 | Breadboards, 830 holes               | 4      | 6  | one per node + a spare         |
| 14 | Jumpers M-M and M-F                  | 2 kits | 4  | 40 pieces of each type         |
| 15 | Resistor kit                         | 1      | 3  | 10 kΩ (the divider) and others |
| 16 | USB-C data cables (not charge-only!) | 3      | 6  | flashing all three nodes       |
| 17 | 5 V 2 A supply (USB or terminals)    | 2      | 6  | one for the servo, one spare   |

## 8. Optional / stretch

| №     | Component                             | Qty | ~$ | Why                                                                    |
| ----- | ------------------------------------- | --- | -- | ---------------------------------------------------------------------- |
| 18    | OV2640 camera (see the verdict below) | —   | 5  | closed by board №18alt (the camera is bundled)                         |
| 18alt | ESP32-S3 WROOM N16R8 CAM + OV2640     | 1   | 15 | the right way to the camera — **bought**, node P + the reject detector |
| 19    | Logic analyzer                        | 1   | 10 | a 24 MHz Saleae clone: debugging the I2S/IR pins                       |
| 20    | Multimeter                            | 1   | —  | if not at home (de facto mandatory)                                    |

### The item 18 verdict: OV2640 + DevKitC-1

**Decision (2026-08-20): board №18alt is bought — an ESP32-S3 WROOM N16R8 CAM
with a bundled OV2640; the separate camera item 18 is not ordered.** The
reasoning below is kept as the original analysis.

- **Physics**: the DevKitC-1 has no camera socket; the OV2640 ships with an
  18-pin 0.5 mm FPC ribbon for boards like the ESP32-CAM / XIAO ESP32S3
  Sense. On a DevKitC-1 — only an FPC adapter or a rare module with headers +
  ~18 jumpers (D0–D7, PCLK, VSYNC, HREF, XCLK, SIOD/SIOC, 3V3, GND, PWDN,
  RESET).
- **Software (the main thing)**: in no_std `esp-hal` (checked on docs.rs,
  v1.1.2) there is no ready DVP-camera driver — only gpio/i2c/i2s/spi/parl_io
  etc.; the driver would have to be written from scratch. The ready
  `esp32-camera` exists only for the esp-idf/std path.
- **Signal-wise** the S3 can do DVP (GPIO matrix, XCLK ~20 MHz generated by
  the chip itself) — "theoretically possible, practically not worth it."
  GPIO35–37 are taken by PSRAM, but the remaining pins suffice.
- **If the camera becomes real**: buy an ESP32-S3-CAM (~$10–15, an OV2640
  bundled and already wired to the board) or a XIAO ESP32S3 Sense and work
  through esp-idf/std — not a DevKitC-1 + ribbon + a hand-written driver.

## 9. S3 pin cheat sheet (to avoid the rakes)

| Resource        | Pins         | Use in the project                                  |
| --------------- | ------------ | --------------------------------------------------- |
| ADC1            | GPIO1–GPIO10 | the ACS712 input → GPIO4 (after the divider)        |
| I2S (matrix)    | any GPIO     | SCK GPIO12, WS GPIO13, SD GPIO14                    |
| PWM (servo)     | any GPIO     | GPIO11                                              |
| GPIO input (IR) | any GPIO     | GPIO5 (on the CAM board — a pin free of the camera) |

Do not occupy: **GPIO19–20** (USB D−/D+), **GPIO35–37** (taken by the octal
PSRAM on the N16R8!), GPIO43–44 (UART0/console), GPIO0/3/45/46 (strapping).

## 10. Ordering order

1. **Day 0** — everything "mandatory" in one order (AliExpress delivery
   2–4 weeks; Ozon/WB — in 1–2 days if the deadline burns).
2. Camera №18 and analyzer №19 can be deferred until the stretch decision.
3. On arrival — run every board: blink + a WiFi scan (5 minutes per board);
   mark the dead ones with a marker right away, spare N16R8 boards are
   cheaper than a second delivery.
4. Module check on arrival: `esptool.py flash_id` — shows the real flash
   size (16 MB) and the PSRAM; an N8R2 slipped in shows 2 MB quad — visible
   at once.

## 11. Ozon links (search by the exact query)

Ozon blocks automated access (anti-bot), and direct lot links rot within
weeks — hence the search links; pick the lot by the rules below.

| №     | Component                           | Ozon search                                                                      | In cart    |
| ----- | ----------------------------------- | -------------------------------------------------------------------------------- | ---------- |
| 1     | ESP32-S3-DevKitC-1 N16R8            | [search](https://www.ozon.ru/search/?text=ESP32-S3-DevKitC-1+N16R8)              | ✅ 2 pcs   |
| 2     | ACS712-20A                          | [search](https://www.ozon.ru/search/?text=ACS712+20A+модуль)                     | ✅ 1 pc    |
| 3     | INA226                              | [search](https://www.ozon.ru/search/?text=INA226+модуль)                         | ✅ 1 pc    |
| 4, 15 | Resistor kit                        | [search](https://www.ozon.ru/search/?text=набор+резисторов+для+ардуино)          | ✅ 1 pc    |
| 5     | INMP441                             | [search](https://www.ozon.ru/search/?text=INMP441+микрофон)                      | ✅ 1 pc    |
| 6     | SG90 servo                          | [search](https://www.ozon.ru/search/?text=сервопривод+SG90)                      | ✅ 1 pc    |
| 7     | 470 µF capacitor                    | [search](https://www.ozon.ru/search/?text=конденсатор+470+мкФ+6.3В)              | ✅ 5 pcs   |
| 8     | Skewers (the tapper)                | [search](https://www.ozon.ru/search/?text=шпажки+деревянные)                     | ✅ 100 pcs |
| 9     | TCRT5000                            | [search](https://www.ozon.ru/search/?text=TCRT5000+модуль)                       | ✅ 2 pcs   |
| 10    | Nuts/washers M5–M8                  | [search](https://www.ozon.ru/search/?text=набор+гаек+шайб+М5+М6+М8)              | 🏠 have    |
| 11    | DIY mini conveyor                   | [search](https://www.ozon.ru/search/?text=mini+conveyor+belt+arduino)            | ✅ 1 pc    |
| 13    | Breadboard, 830 holes               | [search](https://www.ozon.ru/search/?text=макетная+плата+MB-102+830)             | ✅ 1 pc    |
| 14a   | Jumpers male-male                   | [search](https://www.ozon.ru/search/?text=провода+перемычки+папа-папа+40+шт)     | ✅ 40 pcs  |
| 14b   | Jumpers male-female                 | [search](https://www.ozon.ru/search/?text=провода+перемычки+папа-мама+40+шт)     | ✅ 40 pcs  |
| 16    | USB-C cable (data!)                 | [search](https://www.ozon.ru/search/?text=кабель+USB-C+для+передачи+данных+1м)   | 🏠 have    |
| 17    | 5 V 2 A supply                      | [search](https://www.ozon.ru/search/?text=блок+питания+5В+2А+разъем)             | —          |
| 18    | OV2640 (see the section 8 verdict!) | [search](https://www.ozon.ru/search/?text=OV2640+камера)                         | —          |
| 18alt | ESP32-S3-CAM (the better one)       | [search](https://www.ozon.ru/search/?text=ESP32-S3-CAM)                          | ✅ 1 pc    |
| 19    | 24 MHz logic analyzer               | [search](https://www.ozon.ru/search/?text=логический+анализатор+24MHz+8+каналов) | —          |
| 20    | DT-830B multimeter                  | [search](https://www.ozon.ru/search/?text=мультиметр+DT-830B)                    | 🏠 have    |

"🏠" — already at home, not ordered: the USB-C cables (№16), the multimeter
(№20), the M5–M8 nuts/washers (№10). All critical positions are closed: the
cart (the boards, the sensors, a breadboard, jumpers, resistors, five 470 µF
16 V capacitors, one hundred 30 cm skewers, a 100×350 mm conveyor with a
manual 0–116 rpm speed control — check the bundled adapter and its voltage).
Check the home cables №16 that they are data, not charge-only (rule 3). The
skewers: cut the tapper to ~10–15 cm (30 cm springs), hot glue / a zip tie
to the SG90 horn. Cart status as of 2026-08-20: the CAM board covers node P
(IR) + the stretch camera, the DevKitC-1 ×2 — nodes A and Q. Four
breadboards were planned — with 1 pc, assemble the nodes one at a time.

Lot-picking rules (the main traps):

1. **№1 — the most important check**: `N16R8` explicitly in the title/on the
   photo. If it just says "ESP32-S3-DevKitC-1" — ask the seller "which
   module is on the board?" before ordering (a frequent swap to the N8R2).
2. Seller rating ≥ 4.8 and ≥ 50 reviews on the lot; in board reviews look
   for "flashes/esp-idf" — means working units do arrive.
3. №16: a smartphone cable is surely data; a "charging cable" for 100 ₽ is
   often charge-only, won't flash.
4. №19: search for "USB Logic Analyzer 24MHz 8ch" (a Saleae clone); not
   always on Ozon, the fallback is AliExpress.
5. Ozon prices are 30–50% above AliExpress — the price of 1–2 day delivery
   instead of 2–4 weeks.
6. №3 (INA226): look at the shunt marking on the board photo — `R100`
   (0.1 Ω, a ±0.82 A limit — ideal for a fan) or `R010` (0.01 Ω, ±8.2 A —
   more universal); the shunt goes only into a low-voltage circuit ≤36 V
   (the fan/drill supply), NEVER into 220 V.

Related documents: the code-only plan [`plan.md`](plan.md); the hardware
plan and the short purchase list live in `kontext/` (working notes).
