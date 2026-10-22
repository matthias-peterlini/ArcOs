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

/// The kernel is linked at `KERNEL_BASE + 1 MiB`; the BIOS loader puts it
/// at physical 1 MiB, the UEFI loader wherever the firmware had room (at
/// the same offset within a 2 MiB page).
pub const KERNEL_LINK_OFFSET: u64 = 0x10_0000;
static KERNEL_PHYS: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(KERNEL_LINK_OFFSET);

/// Physical address the kernel image was loaded at.
pub fn kernel_phys() -> u64 {
    KERNEL_PHYS.load(core::sync::atomic::Ordering::Relaxed)
}

/// Physical range of the loaded kernel image (including .bss).
pub fn kernel_range() -> (u64, u64) {
    let (s, e) = (core::ptr::addr_of!(__kernel_start) as u64, core::ptr::addr_of!(__kernel_end) as u64);
    let base = kernel_phys() - KERNEL_LINK_OFFSET;
    (s - KERNEL_BASE + base, e - KERNEL_BASE + base)
}

/// Sets up physical and virtual memory and the heap.
pub fn init(boot: &crate::boot::BootInfo) {
    if boot.kernel_phys != 0 {
        KERNEL_PHYS.store(boot.kernel_phys, core::sync::atomic::Ordering::Relaxed);
    }
    frame::init(boot);
    paging::init();
    heap::init();
}
