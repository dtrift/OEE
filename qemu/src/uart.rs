//! LM3S6965 UART0 driver (the QEMU `lm3s6965evb` machine).
//!
//! The board's UART0 is wired to the QEMU serial backend (`-nographic`
//! redirects it to stdio), so a `write_str` here lands in the demo log.
//!
//! Deliberately minimal (week 6, D2): register-level TX with busy-wait on
//! the FIFO flag — no RX, no IRQs, no baud/clock setup. QEMU boots the
//! lm3s6965evb with UART0 already clocked and connected; real silicon would
//! additionally need the GPIOA pin mux and the UART enable sequence (see
//! the report's limitations — this firmware targets the emulator).

use core::fmt;

/// UART0 peripheral base (LM3S6965 datasheet, "UARTs": 0x4000.C000).
const UART0_BASE: usize = 0x4000_C000;

/// Data register offset (byte access transmits).
const DR: usize = 0x000;

/// Flag register offset.
const FR: usize = 0x018;

/// FR bit 5: transmit FIFO full.
const FR_TXFF: u32 = 1 << 5;

/// The UART0 handle.
pub struct Uart0;

impl Uart0 {
    /// Blocks until the transmit FIFO has room, then queues one byte.
    pub fn write_byte(&mut self, byte: u8) {
        while unsafe { read_reg(FR) } & FR_TXFF != 0 {}
        unsafe { write_reg(DR, byte as u32) };
    }

    /// Sends a newline-terminated line (convenience for demo output).
    pub fn write_line(&mut self, line: &str) {
        for byte in line.as_bytes() {
            self.write_byte(*byte);
        }
        self.write_byte(b'\n');
    }
}

impl fmt::Write for Uart0 {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.as_bytes() {
            self.write_byte(*byte);
        }
        Ok(())
    }
}

/// Reads a 32-bit register of UART0.
///
/// # Safety
/// UART0 exists on the target and is never aliased to Rust state.
unsafe fn read_reg(offset: usize) -> u32 {
    ((UART0_BASE + offset) as *const u32).read_volatile()
}

/// Writes a 32-bit register of UART0.
///
/// # Safety
/// See [`read_reg`].
unsafe fn write_reg(offset: usize, value: u32) {
    ((UART0_BASE + offset) as *mut u32).write_volatile(value)
}
