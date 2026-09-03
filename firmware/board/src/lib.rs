#![no_std]

//! OEE bench pins. Boards: 2× ESP32-S3-DevKitC-1 (N16R8) — nodes A and Q;
//! 1× ESP32-S3-WROOM-1 N16R8 CAM (OV2640 on board) — node P plus the
//! stretch camera (its camera wiring takes some pins — cross-check the
//! board's schematic before reusing them).
//!
//! Single source of truth for wiring. Bench schematic changes are made
//! here, not in node firmwares.

/// Node A (current): ACS712-20A through a 2:1 divider.
pub mod node_a {
    /// ACS712 input after the divider — ADC1, GPIO4.
    ///
    /// ADC1 only: ADC2 conflicts with WiFi (MQTT over WiFi).
    pub const ADC_CURRENT: u8 = 4;
}

/// Node Q (audio + servo).
pub mod node_q {
    /// I2S SCK (BCLK) of the INMP441 mic.
    pub const I2S_SCK: u8 = 12;
    /// I2S WS (LRCL).
    pub const I2S_WS: u8 = 13;
    /// I2S SD (mic data).
    pub const I2S_SD: u8 = 14;
    /// SG90 servo PWM, 50 Hz. Servo power is a separate 5 V supply.
    pub const SERVO_PWM: u8 = 11;
}

/// Node P (IR barrier, on the CAM board): TCRT5000 module OUT (comparator
/// on the module, no resistors needed).
pub mod node_p {
    /// IR-barrier output; edge counting, ~50 ms debounce. On the CAM board,
    /// keep this pin free of the camera wiring (check the board schematic —
    /// GPIO5 is the DevKitC-1 assignment).
    pub const IR_OUT: u8 = 5;
}

/// Pins taken by the board or chip: do not use in bench schematics.
pub const RESERVED: [u8; 11] = [
    0, 3, 45, 46, // strapping
    19, 20, // USB D−/D+
    43, 44, // UART0/console
    35, 36, 37, // octal PSRAM on N16R8
];

#[cfg(test)]
mod tests {
    use super::*;

    /// All assigned pins are distinct and avoid the reserved ones.
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
            assert!(
                !RESERVED.contains(&pin),
                "pin {pin} is reserved by the board"
            );
            assert!(
                !used[..i].contains(&pin),
                "pin {pin} assigned twice — node conflict"
            );
        }
    }
}
