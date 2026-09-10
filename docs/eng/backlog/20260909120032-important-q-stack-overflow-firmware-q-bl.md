# Likely ESP32 main-task stack overflow in node Q

- **Severity**: important
- **Component**: firmware-q
- **Source**: code review `backlog/20260909120000-review.md` (Russian original), appendix A6, Important #4
- **Location**: `firmware/q/src/main.rs:99` + `firmware/q/src/lib.rs:31-45`; buffer estimates — `ml/models/model_q.ops.txt`

## Summary

The window `[f32; 1024]` = 4 KiB on the stack; in `classify` it is copied into an `SMatrix` (another 4 KiB); `predict` for model_q needs the layer int8 buffers:

- `conv0_out` 1022×8 = 8176 B;
- `conv2_out` 509×16 = 8144 B;
- `pool1`/`pool3` another ~8 KiB.

Peak usage is plausibly 12–20+ KiB against the default esp-hal main-task stack. The CI `firmware` job is build-only and will not catch this: you will learn it on the hardware as a hang.

Node A (128×4 = 512 B) has headroom.

## Suggested fix

- Raise the main-task stack size via `esp_hal::Config` (check the exact method name for your esp-hal version);
- and/or make the window `static` (`StaticCell`/`static mut` with justification);
- check the high-water mark at bring-up, before moving to real I2S (DMA buffers will add on top).
