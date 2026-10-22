//! User programs (ring 3). Placeholder until the program loader lands.

use crate::arch::idt::TrapFrame;

pub fn syscall(frame: &mut TrapFrame) {
    frame.rax = u64::MAX;
}

pub fn fault(frame: &mut TrapFrame, what: &str) {
    crate::console::panic_screen(format_args!("user program fault: {what} at {:#x}", frame.rip));
}
