# Hardware Purchase (optional: the plan is code-only)

> Context: the approved plan ([`plan.md`](plan.md)) is hardware-free, the
> metrics come through QEMU. This list is for a live defense demo and/or
> hardware future work right after the course. The project's architecture
> (`trait SensorSource → SimSource / AdcSource`) is ready for the move to
> hardware.

## Bought (2026-08-20): 2 × ESP32-S3-DevKitC-1 N16R8 + 1 × ESP32-S3 WROOM N16R8 CAM (OV2640)

| Qty | Board                             | Role                 | Why                                             |
| --- | --------------------------------- | -------------------- | ----------------------------------------------- |
| 1   | S3-DevKitC-1 N16R8                | Node A (current)     | ADC1 + WiFi (MQTT) at the same time             |
| 1   | S3-DevKitC-1 N16R8                | Node Q (microphone)  | I2S for the INMP441 + the servo                 |
| 1   | S3-WROOM-1 N16R8 **CAM** + OV2640 | Node P (IR counting) | the simplest role + the camera already on board |

All three boards are ESP32-S3 N16R8 (one toolchain, the firmwares are
interchangeable; the single limitation — on the CAM board some pins are
taken by the camera). The original recommendation was "3 × DevKitC-1"; at
purchase the third board was swapped for the CAM variant: the OV2640 comes
"free" (a ~$5 difference), and any of the three boards can serve as a spare
for the A/Q firmwares.

## Budget variant B (history — not chosen)

2 × ESP32 DevKitV1 (WROOM-32) for nodes A and Q + 1 × S3 N16R8
(camera/spare):

- Saves ~$10, but loses interchangeability.
- The classic ESP32s have no PSRAM; a camera on them is noticeably harder.

## What to check when ordering

1. **The module marking**: `ESP32-S3-WROOM-1-N16R8` — exactly **N16R8**
   (16 MB flash + 8 MB octal PSRAM). Sellers often ship an N8R2 (2 MB
   PSRAM, quad) — too little for the camera and the spare role. The marking
   is printed on the module's metal can.
2. **Two USB-C ports** on the board — the native USB-OTG and a UART bridge;
   both working, if one won't come up — flash through the other.
3. AliExpress clones work fine; the reference is a board photo with the
   `ESP32-S3-DevKitC-1` silkscreen.
4. Price: the original ~$12–15, clones ~$8–12 apiece; delivery 2–4 weeks —
   order on the day of the "we do hardware" decision.

## Companion components

| Component                        | Node | Note                                                           |
| -------------------------------- | ---- | -------------------------------------------------------------- |
| ACS712-20A                       | A    | a 5 V sensor: on the S3 a voltage divider is mandatory         |
| INA226                           | A    | the divider-free ACS712 alternative (I2C, 3.3 V), neater       |
| INMP441                          | Q    | an I2S microphone, wires to the S3 directly                    |
| SG90 + a stick                   | Q    | the tapper for the acoustic test                               |
| TCRT5000 ×2                      | P    | part counting + "belt end"                                     |
| OV2640                           | P    | the stretch camera — already on the CAM board (bought with it) |
| Breadboards, wires, a 5 V supply | all  | the common miscellany                                          |

## Reference: how the S3 differs from the classic ESP32

- The ESP32-S3 is absent from MicroFlow's officially tested MCU list, but
  that is not a problem: the engine is pure allocation-free, platform-agnostic
  `no_std` Rust, and the S3 is the same Xtensa (LX7) as the classic ESP32
  (LX6) that is on the list. There is no ready S3 example in the repo — one
  is written from scratch; `#[model]` does not depend on the board.
- Toolchain: the S3 is Xtensa, needs `espup` (a patched Rust). The
  patch-free alternative is the ESP32-C3 (RISC-V, stock Rust), but the C3
  has less RAM and no camera options.
- ACS712: the output at zero current is 2.5 V, above 3.3 V at drill
  currents, and the S3's GPIOs are not 5 V-tolerant — the divider is
  mandatory. For node A use **ADC1** pins (ADC2 conflicts with WiFi, and our
  MQTT runs over WiFi).
- An S3 bonus: vector (SIMD) instructions — a potential future work on
  speeding up the int8 `Conv1D` kernels through vectorization.
