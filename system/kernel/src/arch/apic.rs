//! The local APIC, used only to receive message-signalled interrupts (MSI):
//! legacy device lines and the timer stay on the 8259 PIC.

use crate::mem::{paging, region};
use crate::sync::IrqLock;

const MSR_APIC_BASE: u32 = 0x1B;
const REG_ID: usize = 0x20;
const REG_EOI: usize = 0xB0;
const REG_SVR: usize = 0xF0;
const REG_LINT0: usize = 0x350;
const REG_LINT1: usize = 0x360;

/// Vector the APIC uses for spurious interrupts (ignored, no EOI).
pub const VEC_SPURIOUS: u8 = 0xFF;

/// (virtual address of the registers, APIC id); 0 until enabled.
static APIC: IrqLock<(u64, u32)> = IrqLock::new((0, 0));

fn read(base: u64, reg: usize) -> u32 {
    // SAFETY: inside the mapped register page.
    unsafe { core::ptr::read_volatile((base as usize + reg) as *const u32) }
}

fn write(base: u64, reg: usize, v: u32) {
    // SAFETY: inside the mapped register page.
    unsafe { core::ptr::write_volatile((base as usize + reg) as *mut u32, v) }
}

/// Enables the local APIC (once) and returns its id, or `None` if the CPU
/// has none.
pub fn enable() -> Option<u32> {
    let mut a = APIC.lock();
    if a.0 != 0 {
        return Some(a.1);
    }
    if super::cpu::cpuid(1, 0).edx & (1 << 9) == 0 {
        return None;
    }
    let msr = super::cpu::rdmsr(MSR_APIC_BASE);
    let phys = msr & 0xF_FFFF_F000;
    if msr & (1 << 11) == 0 {
        // SAFETY: turns on the APIC at its current base.
        unsafe { super::cpu::wrmsr(MSR_APIC_BASE, msr | (1 << 11)) };
    }
    let base = region::map_device(phys, 4096, paging::NO_CACHE)?;
    let svr = read(base, REG_SVR);
    if svr & 0x100 == 0 {
        // Software-disabled until now: the PIC keeps working through LINT0
        // (virtual wire mode), NMIs through LINT1.
        write(base, REG_LINT0, 0x700);
        write(base, REG_LINT1, 0x400);
    }
    write(base, REG_SVR, svr | 0x100 | VEC_SPURIOUS as u32);
    let id = read(base, REG_ID) >> 24;
    *a = (base, id);
    Some(id)
}

/// Ends an APIC interrupt.
pub fn eoi() {
    let base = APIC.lock().0;
    if base != 0 {
        write(base, REG_EOI, 0);
    }
}
