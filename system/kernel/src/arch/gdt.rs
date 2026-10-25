//! Global descriptor table and task state segment.
//!
//! Selector layout (fixed by `syscall`/`sysret`):
//! `0x08` kernel code, `0x10` kernel data, `0x18` user data, `0x20` user code,
//! `0x28` TSS.

use core::arch::asm;
use core::cell::UnsafeCell;

pub const KERNEL_CODE: u16 = 0x08;
pub const KERNEL_DATA: u16 = 0x10;
pub const USER_DATA: u16 = 0x18 | 3;
pub const USER_CODE: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

#[repr(C, packed)]
struct Tss {
    _reserved0: u32,
    rsp: [u64; 3],
    _reserved1: u64,
    ist: [u64; 7],
    _reserved2: u64,
    _reserved3: u16,
    iomap_base: u16,
}

#[repr(C, align(16))]
struct Stack([u8; 16384]);

struct Tables {
    gdt: [u64; 7],
    tss: Tss,
}

struct Global(UnsafeCell<Tables>);
// SAFETY: written during single-threaded init and by the scheduler with
// interrupts disabled.
unsafe impl Sync for Global {}

static TABLES: Global = Global(UnsafeCell::new(Tables {
    gdt: [0; 7],
    tss: Tss { _reserved0: 0, rsp: [0; 3], _reserved1: 0, ist: [0; 7], _reserved2: 0, _reserved3: 0, iomap_base: 104 },
}));

static mut DOUBLE_FAULT_STACK: Stack = Stack([0; 16384]);

/// Interrupt stack table slot used by the double-fault handler.
pub const IST_DOUBLE_FAULT: u8 = 1;

#[repr(C, packed)]
struct Pointer {
    limit: u16,
    base: u64,
}

pub fn init() {
    // SAFETY: single-threaded early boot.
    let t = unsafe { &mut *TABLES.0.get() };
    t.gdt[1] = 0x00AF_9A00_0000_FFFF; // kernel code: present, ring 0, 64-bit
    t.gdt[2] = 0x00CF_9200_0000_FFFF; // kernel data
    t.gdt[3] = 0x00CF_F200_0000_FFFF; // user data: ring 3
    t.gdt[4] = 0x00AF_FA00_0000_FFFF; // user code: ring 3, 64-bit

    let stack_top = core::ptr::addr_of!(DOUBLE_FAULT_STACK) as u64 + 16384;
    t.tss.ist[IST_DOUBLE_FAULT as usize - 1] = stack_top;

    let base = core::ptr::addr_of!(t.tss) as u64;
    let limit = (core::mem::size_of::<Tss>() - 1) as u64;
    t.gdt[5] = limit & 0xFFFF
        | (base & 0xFF_FFFF) << 16
        | 0x89 << 40 // present, 64-bit available TSS
        | (limit >> 16 & 0xF) << 48
        | (base >> 24 & 0xFF) << 56;
    t.gdt[6] = base >> 32;

    let ptr = Pointer { limit: (core::mem::size_of_val(&t.gdt) - 1) as u16, base: t.gdt.as_ptr() as u64 };
    // SAFETY: the tables are static and valid; reloads every segment register.
    unsafe {
        asm!(
            "lgdt [{ptr}]",
            "push {code}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            "mov ds, {data:x}",
            "mov es, {data:x}",
            "mov ss, {data:x}",
            "xor {tmp:e}, {tmp:e}",
            "mov fs, {tmp:x}",
            "mov gs, {tmp:x}",
            "ltr {tss:x}",
            ptr = in(reg) &ptr,
            code = in(reg) KERNEL_CODE as u64,
            data = in(reg) KERNEL_DATA as u64,
            tss = in(reg) TSS_SELECTOR as u64,
            tmp = out(reg) _,
        );
    }
}

/// Stack the CPU switches to when an interrupt arrives from user mode.
pub fn set_kernel_stack(top: u64) {
    // SAFETY: called with interrupts disabled; a plain store.
    unsafe { (*TABLES.0.get()).tss.rsp[0] = top };
    super::syscall::set_kernel_stack(top);
}
