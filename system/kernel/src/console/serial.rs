//! COM1 (16550 UART): the debug console. Output mirrors the screen; input
//! feeds the same queue as the keyboard.

use crate::arch::cpu::{inb, outb};
use core::sync::atomic::{AtomicBool, Ordering};

const COM1: u16 = 0x3F8;
static PRESENT: AtomicBool = AtomicBool::new(false);

pub fn init() {
    outb(COM1 + 1, 0x00); // no interrupts yet
    outb(COM1 + 3, 0x80); // DLAB
    outb(COM1, 0x01); // divisor 1: 115200 baud
    outb(COM1 + 1, 0x00);
    outb(COM1 + 3, 0x03); // 8N1
    outb(COM1 + 2, 0xC7); // FIFO on, cleared, 14-byte threshold
    outb(COM1 + 4, 0x1E); // loopback to test the chip
    outb(COM1, 0xAE);
    if inb(COM1) != 0xAE {
        return;
    }
    outb(COM1 + 4, 0x0F); // normal operation, OUT2 (IRQ) on
    PRESENT.store(true, Ordering::Relaxed);
}

pub fn present() -> bool {
    PRESENT.load(Ordering::Relaxed)
}

pub fn write_byte(b: u8) {
    if !present() {
        return;
    }
    let mut spins = 0;
    while inb(COM1 + 5) & 0x20 == 0 && spins < 100_000 {
        spins += 1;
        core::hint::spin_loop();
    }
    outb(COM1, b);
}

pub fn write(bytes: &[u8]) {
    for &b in bytes {
        if b == b'\n' {
            write_byte(b'\r');
        }
        write_byte(b);
    }
}

pub fn read_byte() -> Option<u8> {
    (present() && inb(COM1 + 5) & 1 != 0).then(|| inb(COM1))
}

/// Interrupt on received data.
pub fn enable_rx_irq() {
    if present() {
        outb(COM1 + 1, 0x01);
    }
}

pub fn irq_handler() {
    while let Some(b) = read_byte() {
        crate::dev::input::serial_byte(b);
    }
}
