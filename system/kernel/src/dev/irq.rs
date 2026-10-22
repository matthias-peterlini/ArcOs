//! Interrupt lines shared between kernel handlers and driver modules.

use crate::arch::pic;
use crate::sync::IrqLock;
use alloc::vec::Vec;
use arcdk::IrqHandler;
use core::ffi::c_void;

#[derive(Clone, Copy)]
enum Handler {
    Kernel(fn()),
    Driver(IrqHandler, *mut c_void),
}

#[derive(Clone, Copy)]
struct Entry {
    owner: u32,
    handler: Handler,
}

// SAFETY: driver contexts are only used by their own handler.
unsafe impl Send for Entry {}

static LINES: IrqLock<[Vec<Entry>; 16]> = IrqLock::new([const { Vec::new() }; 16]);
static COUNTS: IrqLock<[u64; 16]> = IrqLock::new([0; 16]);

/// Owner id of handlers that belong to the kernel itself.
pub const KERNEL: u32 = 0;

pub fn attach_kernel(irq: u8, handler: fn()) {
    attach(irq, Entry { owner: KERNEL, handler: Handler::Kernel(handler) });
}

pub fn attach_driver(device: u32, irq: u8, handler: IrqHandler, ctx: *mut c_void) -> bool {
    if irq >= 16 || irq == 2 {
        return false;
    }
    attach(irq, Entry { owner: device, handler: Handler::Driver(handler, ctx) });
    true
}

fn attach(irq: u8, entry: Entry) {
    let mut lines = LINES.lock();
    lines[irq as usize].push(entry);
    pic::unmask(irq);
}

/// Removes the handlers of `device` (on `irq`, or on every line for `None`).
pub fn detach(device: u32, irq: Option<u8>) {
    let mut lines = LINES.lock();
    for (line, list) in lines.iter_mut().enumerate() {
        if irq.is_some_and(|i| i as usize != line) {
            continue;
        }
        let before = list.len();
        list.retain(|e| e.owner != device);
        if list.is_empty() && before != 0 {
            pic::mask(line as u8);
        }
    }
}

/// Runs every handler of a line (called with interrupts disabled).
pub fn dispatch(irq: u8) {
    COUNTS.lock()[irq as usize] += 1;
    let handlers: Vec<Entry> = LINES.lock()[irq as usize].clone();
    for e in handlers {
        match e.handler {
            Handler::Kernel(f) => f(),
            // SAFETY: the driver registered this handler with this context.
            Handler::Driver(f, ctx) => unsafe { f(ctx) },
        }
    }
}

/// (line, interrupts so far, handler owners) for every line in use.
pub fn stats() -> Vec<(u8, u64, Vec<u32>)> {
    let lines = LINES.lock();
    let counts = COUNTS.lock();
    (0..16u8)
        .filter(|&i| counts[i as usize] != 0 || !lines[i as usize].is_empty())
        .map(|i| (i, counts[i as usize], lines[i as usize].iter().map(|e| e.owner).collect()))
        .collect()
}
