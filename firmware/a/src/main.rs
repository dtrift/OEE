//! Прошивка узла A (Availability): ACS712-20A → ADC1 (GPIO4, [`board`]) →
//! калибровка `features-cli::calibration` (нуль — усреднением на холостом
//! ходу при старте) → окно 128 @ 1.6 кГц (`features-cli::window_spec`) →
//! `predict()` → статус.
//!
//! Скелет: собирается на хосте без esp-тулчейна. План первой версии:
//! статусы в USB-CDC (та же схема CSV, что в `features-cli::capture`),
//! MQTT через esp-wifi — после end-to-end по проводу.

fn main() {
    // TODO(обкатка): #![no_std] + esp-hal; шаги — ../README.md.
}
