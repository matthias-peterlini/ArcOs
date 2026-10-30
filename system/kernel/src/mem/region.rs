//! Virtual address ranges in the kernel half: device memory, module images
//! and thread stacks.

use super::{frame, paging, PAGE_SIZE};
use crate::sync::IrqLock;
use alloc::vec::Vec;

pub struct Region {
    next: u64,
    end: u64,
    /// Freed ranges (start, pages), reused first-fit.
    free: Vec<(u64, u64)>,
}

impl Region {
    pub const fn new(base: u64, size: u64) -> Region {
        Region { next: base, end: base + size, free: Vec::new() }
    }

    fn take(&mut self, pages: u64) -> Option<u64> {
        if let Some(i) = self.free.iter().position(|&(_, n)| n >= pages) {
            let (start, n) = self.free[i];
            if n == pages {
                self.free.swap_remove(i);
            } else {
                self.free[i] = (start + pages * PAGE_SIZE, n - pages);
            }
            return Some(start);
        }
        let start = self.next;
        if start + pages * PAGE_SIZE > self.end {
            return None;
        }
        self.next += pages * PAGE_SIZE;
        Some(start)
    }

    fn give_back(&mut self, start: u64, pages: u64) {
        self.free.push((start, pages));
    }
}

pub static MMIO: IrqLock<Region> = IrqLock::new(Region::new(super::MMIO_BASE, super::REGION_SIZE));
pub static MODULES: IrqLock<Region> = IrqLock::new(Region::new(super::MODULE_BASE, super::REGION_SIZE));
pub static STACKS: IrqLock<Region> = IrqLock::new(Region::new(super::STACK_BASE, super::REGION_SIZE));

/// Maps device memory. `flags` is [`paging::NO_CACHE`] or
/// [`paging::WRITE_COMBINE`].
pub fn map_device(phys: u64, size: u64, flags: u64) -> Option<u64> {
    let base = super::align_down(phys, PAGE_SIZE);
    let pages = (super::align_up(phys + size, PAGE_SIZE) - base) / PAGE_SIZE;
    let virt = MMIO.lock().take(pages)?;
    for i in 0..pages {
        paging::map(virt + i * PAGE_SIZE, base + i * PAGE_SIZE, paging::WRITE | flags)?;
    }
    Some(virt + (phys - base))
}

/// Maps `pages` fresh zeroed pages in `region`, preceded by `guard` unmapped
/// pages (a stack overflow then faults instead of corrupting memory).
pub fn alloc_pages(region: &IrqLock<Region>, pages: u64, guard: u64) -> Option<u64> {
    let start = region.lock().take(pages + guard)?;
    let virt = start + guard * PAGE_SIZE;
    for i in 0..pages {
        let Some(phys) = frame::alloc() else {
            free_pages(region, virt, i, guard);
            return None;
        };
        paging::map(virt + i * PAGE_SIZE, phys, paging::WRITE)?;
    }
    Some(virt)
}

/// Undoes [`alloc_pages`]: unmaps the pages and frees their frames.
pub fn free_pages(region: &IrqLock<Region>, virt: u64, pages: u64, guard: u64) {
    let frames: Vec<u64> = (0..pages).filter_map(|i| paging::unmap(virt + i * PAGE_SIZE)).collect();
    // No processor may still reach the frames through its TLB once they
    // are reused.
    crate::arch::smp::shootdown();
    for phys in frames {
        frame::free(phys);
    }
    region.lock().give_back(virt - guard * PAGE_SIZE, pages + guard);
}
