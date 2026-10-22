//! The kernel heap: an address-ordered free list with coalescing, growing on
//! demand into the heap region.
//!
//! Every block start and size is a multiple of 16, so splitting a free block
//! never leaves a fragment too small to describe itself.

use super::{align_up, frame, paging, HEAP_BASE, HEAP_MAX, PAGE_SIZE};
use crate::sync::IrqLock;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;

const UNIT: usize = 16;
const GROW_MIN: u64 = 1 << 20;

struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

struct Heap {
    head: *mut FreeBlock,
    top: u64,
    used: usize,
}

// SAFETY: only reached through the lock.
unsafe impl Send for Heap {}

static HEAP: IrqLock<Heap> = IrqLock::new(Heap { head: null_mut(), top: HEAP_BASE, used: 0 });

impl Heap {
    /// Inserts a free range, merging it with its neighbours.
    ///
    /// # Safety
    /// The range must be unused heap memory.
    unsafe fn insert(&mut self, addr: usize, size: usize) {
        let mut prev: *mut FreeBlock = null_mut();
        let mut cur = self.head;
        // SAFETY: the list only links valid free blocks.
        unsafe {
            while !cur.is_null() && (cur as usize) < addr {
                prev = cur;
                cur = (*cur).next;
            }
            let block = addr as *mut FreeBlock;
            block.write(FreeBlock { size, next: cur });
            if !cur.is_null() && addr + size == cur as usize {
                (*block).size += (*cur).size;
                (*block).next = (*cur).next;
            }
            if prev.is_null() {
                self.head = block;
            } else if prev as usize + (*prev).size == addr {
                (*prev).size += (*block).size;
                (*prev).next = (*block).next;
            } else {
                (*prev).next = block;
            }
        }
    }

    fn take(&mut self, size: usize, align: usize) -> *mut u8 {
        let mut prev: *mut FreeBlock = null_mut();
        let mut cur = self.head;
        // SAFETY: the list only links valid free blocks.
        unsafe {
            while !cur.is_null() {
                let start = cur as usize;
                let end = start + (*cur).size;
                let aligned = align_up(start as u64, align as u64) as usize;
                if aligned + size <= end {
                    let next = (*cur).next;
                    // Unlink, then give back the front and back remainders.
                    if prev.is_null() {
                        self.head = next;
                    } else {
                        (*prev).next = next;
                    }
                    if aligned > start {
                        self.insert(start, aligned - start);
                    }
                    if aligned + size < end {
                        self.insert(aligned + size, end - aligned - size);
                    }
                    self.used += size;
                    return aligned as *mut u8;
                }
                prev = cur;
                cur = (*cur).next;
            }
        }
        null_mut()
    }

    fn grow(&mut self, at_least: usize) -> bool {
        let bytes = align_up((at_least as u64).max(GROW_MIN), PAGE_SIZE);
        if self.top + bytes > HEAP_BASE + HEAP_MAX {
            return false;
        }
        let start = self.top;
        for i in 0..bytes / PAGE_SIZE {
            let Some(phys) = frame::alloc() else { return false };
            if paging::map(start + i * PAGE_SIZE, phys, paging::WRITE).is_none() {
                return false;
            }
            self.top += PAGE_SIZE;
        }
        // SAFETY: freshly mapped heap memory.
        unsafe { self.insert(start as usize, (self.top - start) as usize) };
        true
    }
}

pub struct KernelAllocator;

// SAFETY: blocks are disjoint, aligned and at least the requested size.
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = align_up(layout.size().max(1) as u64, UNIT as u64) as usize;
        let align = layout.align().max(UNIT);
        let mut heap = HEAP.lock();
        let p = heap.take(size, align);
        if !p.is_null() {
            return p;
        }
        if !heap.grow(size + align) {
            return null_mut();
        }
        heap.take(size, align)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = align_up(layout.size().max(1) as u64, UNIT as u64) as usize;
        let mut heap = HEAP.lock();
        heap.used -= size;
        // SAFETY: the block came from alloc with this layout.
        unsafe { heap.insert(ptr as usize, size) };
    }
}

#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator;

pub fn init() {
    HEAP.lock().grow(4 << 20);
}

/// (bytes in use, bytes mapped)
pub fn stats() -> (usize, usize) {
    let h = HEAP.lock();
    (h.used, (h.top - HEAP_BASE) as usize)
}
