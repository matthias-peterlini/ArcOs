//! The 8254 programmable interval timer: the system tick.

use super::cpu::outb;

pub const HZ: u64 = 1000;
const BASE_HZ: u64 = 1_193_182;

pub fn init() {
    let divisor = (BASE_HZ / HZ) as u16;
    outb(0x43, 0x36); // channel 0, lo/hi byte, mode 3 (square wave)
    outb(0x40, divisor as u8);
    outb(0x40, (divisor >> 8) as u8);
}
