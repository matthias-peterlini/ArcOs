//! The two cascaded 8259 interrupt controllers, remapped to vectors 32..47.

use super::cpu::{inb, io_wait, outb};
use crate::sync::IrqLock;

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

static MASK: IrqLock<u16> = IrqLock::new(0xFFFF);

pub fn init() {
    outb(PIC1_CMD, 0x11); // ICW1: init, ICW4 follows
    io_wait();
    outb(PIC2_CMD, 0x11);
    io_wait();
    outb(PIC1_DATA, 32); // ICW2: vector offsets
    io_wait();
    outb(PIC2_DATA, 40);
    io_wait();
    outb(PIC1_DATA, 4); // ICW3: slave on IRQ 2
    io_wait();
    outb(PIC2_DATA, 2);
    io_wait();
    outb(PIC1_DATA, 1); // ICW4: 8086 mode
    io_wait();
    outb(PIC2_DATA, 1);
    io_wait();
    let mut mask = MASK.lock();
    *mask = !(1 << 2); // everything masked but the cascade
    apply(*mask);
}

fn apply(mask: u16) {
    outb(PIC1_DATA, mask as u8);
    outb(PIC2_DATA, (mask >> 8) as u8);
}

pub fn unmask(irq: u8) {
    let mut mask = MASK.lock();
    *mask &= !(1 << irq);
    apply(*mask);
}

pub fn mask(irq: u8) {
    if irq == 2 {
        return;
    }
    let mut mask = MASK.lock();
    *mask |= 1 << irq;
    apply(*mask);
}

pub fn eoi(irq: u8) {
    if irq >= 8 {
        outb(PIC2_CMD, 0x20);
    }
    outb(PIC1_CMD, 0x20);
}

/// IRQ 7 and 15 can fire spuriously; the in-service register tells.
pub fn is_spurious(irq: u8) -> bool {
    match irq {
        7 => {
            outb(PIC1_CMD, 0x0B);
            inb(PIC1_CMD) & 0x80 == 0
        }
        15 => {
            outb(PIC2_CMD, 0x0B);
            if inb(PIC2_CMD) & 0x80 == 0 {
                outb(PIC1_CMD, 0x20); // the master still saw the cascade
                true
            } else {
                false
            }
        }
        _ => false,
    }
}
