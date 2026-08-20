#![no_std]

//! Пины стенда OEE на ESP32-S3-DevKitC-1 (N16R8).
//!
//! Единый источник истины по подключениям (контекст закупки, разд. 9:
//! `kontext/20260820095259-equipment.md`). Изменение схемы стенда —
//! правка здесь, не в прошивках узлов.

/// Узел A (ток): ACS712-20A через делитель 2:1.
pub mod node_a {
    /// Вход ACS712 после делителя — ADC1, GPIO4.
    ///
    /// Только ADC1: ADC2 конфликтует с WiFi (MQTT по WiFi).
    pub const ADC_CURRENT: u8 = 4;
}

/// Узел Q (звук + серво).
pub mod node_q {
    /// I2S SCK (BCLK) микрофона INMP441.
    pub const I2S_SCK: u8 = 12;
    /// I2S WS (LRCL).
    pub const I2S_WS: u8 = 13;
    /// I2S SD (данные микрофона).
    pub const I2S_SD: u8 = 14;
    /// PWM серво SG90, 50 Гц. Питание серво — отдельный БП 5 В.
    pub const SERVO_PWM: u8 = 11;
}

/// Узел P (IR-барьер): OUT модуля TCRT5000 (компаратор на модуле,
/// резисторы не нужны).
pub mod node_p {
    /// Выход IR-барьера; счёт по фронту, анти-дребезг ~50 мс.
    pub const IR_OUT: u8 = 5;
}

/// Пины, занятые платой или чипом: не использовать в схемах стенда.
pub const RESERVED: [u8; 11] = [
    0, 3, 45, 46, // strapping
    19, 20, // USB D−/D+
    43, 44, // UART0/консоль
    35, 36, 37, // octal PSRAM у N16R8
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Все назначенные пины различны и не попадают в зарезервированные.
    #[test]
    fn used_pins_are_free_and_distinct() {
        let used = [
            node_a::ADC_CURRENT,
            node_q::I2S_SCK,
            node_q::I2S_WS,
            node_q::I2S_SD,
            node_q::SERVO_PWM,
            node_p::IR_OUT,
        ];
        for (i, &pin) in used.iter().enumerate() {
            assert!(!RESERVED.contains(&pin), "пин {pin} зарезервирован платой");
            assert!(
                !used[..i].contains(&pin),
                "пин {pin} назначен дважды — конфликт узлов"
            );
        }
    }
}
