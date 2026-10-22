//! Memory management.
//!
//! Virtual address space (kernel half):
//!
//! | start                  | contents                                   |
//! |------------------------|--------------------------------------------|
//! | `0xFFFF_8000_0000_0000`| direct map of all physical memory          |
//! | `0xFFFF_9000_0000_0000`| kernel heap                                |
//! | `0xFFFF_A000_0000_0000`| device memory (MMIO) mappings              |
//! | `0xFFFF_B000_0000_0000`| driver module images                       |
//! | `0xFFFF_C000_0000_0000`| kernel thread stacks (with guard pages)    |
//! | `0xFFFF_FFFF_8000_0000`| kernel image                               |
//!
//! The lower half belongs to user programs.

pub mod frame;
pub mod heap;
pub mod paging;
pub mod region;

pub const PAGE_SIZE: u64 = 4096;
pub const HHDM: u64 = 0xFFFF_8000_0000_0000;
pub const HEAP_BASE: u64 = 0xFFFF_9000_0000_0000;
pub const HEAP_MAX: u64 = 64 << 30;
pub const MMIO_BASE: u64 = 0xFFFF_A000_0000_0000;
pub const MODULE_BASE: u64 = 0xFFFF_B000_0000_0000;
pub const STACK_BASE: u64 = 0xFFFF_C000_0000_0000;
pub const KERNEL_BASE: u64 = 0xFFFF_FFFF_8000_0000;
pub const REGION_SIZE: u64 = 1 << 40;

#[inline]
pub fn phys_to_virt(phys: u64) -> u64 {
    HHDM + phys
}

/// Physical address of a kernel virtual address, if it is mapped.
pub fn virt_to_phys(virt: u64) -> Option<u64> {
    if (HHDM..HEAP_BASE).contains(&virt) {
        return Some(virt - HHDM);
    }
    paging::translate(virt)
}

pub fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

pub fn align_down(v: u64, a: u64) -> u64 {
    v & !(a - 1)
}

extern "C" {
    static __kernel_start: u8;
    static __kernel_end: u8;
}

/// Physical range of the loaded kernel image (including .bss).
pub fn kernel_range() -> (u64, u64) {
    let (s, e) = (core::ptr::addr_of!(__kernel_start) as u64, core::ptr::addr_of!(__kernel_end) as u64);
    (s - KERNEL_BASE, e - KERNEL_BASE)
}

/// Sets up physical and virtual memory and the heap.
pub fn init(boot: &crate::boot::BootInfo) {
    frame::init(boot);
    paging::init();
    heap::init();
}
