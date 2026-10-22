//! Physical page frame allocator: one bit per 4 KiB frame.

use super::{align_down, align_up, phys_to_virt, PAGE_SIZE};
use crate::boot::{BootInfo, MEM_USABLE};
use crate::sync::IrqLock;

struct Frames {
    bitmap: *mut u64,
    words: usize,
    frames: u64,
    free: u64,
    total_usable: u64,
    hint: u64,
}

// SAFETY: the bitmap is only reached through the lock.
unsafe impl Send for Frames {}

static FRAMES: IrqLock<Frames> =
    IrqLock::new(Frames { bitmap: core::ptr::null_mut(), words: 0, frames: 0, free: 0, total_usable: 0, hint: 0 });

/// Physical memory below this is never handed out (BIOS data, loader).
const LOW_RESERVED: u64 = 0x100000;

impl Frames {
    fn word(&self, i: usize) -> &mut u64 {
        // SAFETY: i < words, checked by callers.
        unsafe { &mut *self.bitmap.add(i) }
    }

    fn is_used(&self, f: u64) -> bool {
        *self.word((f / 64) as usize) & (1 << (f % 64)) != 0
    }

    fn set(&mut self, f: u64, used: bool) {
        let w = self.word((f / 64) as usize);
        let bit = 1u64 << (f % 64);
        if used && *w & bit == 0 {
            *w |= bit;
            self.free -= 1;
        } else if !used && *w & bit != 0 {
            *w &= !bit;
            self.free += 1;
        }
    }

    fn reserve(&mut self, start: u64, end: u64) {
        let first = align_down(start, PAGE_SIZE) / PAGE_SIZE;
        let last = (align_up(end, PAGE_SIZE) / PAGE_SIZE).min(self.frames);
        for f in first..last {
            self.set(f, true);
        }
    }
}

pub fn init(boot: &BootInfo) {
    let map = boot.memory_map();
    let top = map.iter().filter(|r| r.kind == MEM_USABLE).map(|r| r.base + r.len).max().unwrap_or(0);
    let frames = top / PAGE_SIZE;
    let words = frames.div_ceil(64) as usize;
    let bytes = (words * 8) as u64;

    let (k_start, k_end) = super::kernel_range();
    let pack = (boot.pack_phys, boot.pack_phys + boot.pack_size);
    let overlaps = |a: u64, b: u64, c: u64, d: u64| a < d && c < b;

    // Put the bitmap in the first usable RAM above 1 MiB that is free and
    // reachable through the loader's 4 GiB direct map.
    let mut place = None;
    for r in map.iter().filter(|r| r.kind == MEM_USABLE) {
        let mut at = align_up(r.base.max(LOW_RESERVED), PAGE_SIZE);
        let end = r.base + r.len;
        while at + bytes <= end && at + bytes <= 4 << 30 {
            if overlaps(at, at + bytes, k_start, k_end) {
                at = align_up(k_end, PAGE_SIZE);
            } else if overlaps(at, at + bytes, pack.0, pack.1) {
                at = align_up(pack.1, PAGE_SIZE);
            } else {
                place = Some(at);
                break;
            }
        }
        if place.is_some() {
            break;
        }
    }
    let bitmap_phys = place.expect("no room for the frame bitmap");

    let mut f = FRAMES.lock();
    f.bitmap = phys_to_virt(bitmap_phys) as *mut u64;
    f.words = words;
    f.frames = frames;
    for i in 0..words {
        *f.word(i) = u64::MAX;
    }
    f.free = 0;
    // Free the usable regions, then take back what is in use.
    for r in map.iter().filter(|r| r.kind == MEM_USABLE) {
        let first = align_up(r.base, PAGE_SIZE) / PAGE_SIZE;
        let last = align_down(r.base + r.len, PAGE_SIZE) / PAGE_SIZE;
        for fr in first..last.min(frames) {
            f.set(fr, false);
        }
    }
    f.total_usable = f.free;
    // Overlapping firmware entries: anything reported as not usable wins.
    for r in map.iter().filter(|r| r.kind != MEM_USABLE) {
        f.reserve(r.base, r.base + r.len);
    }
    f.reserve(0, LOW_RESERVED);
    f.reserve(k_start, k_end);
    f.reserve(pack.0, pack.1);
    f.reserve(bitmap_phys, bitmap_phys + bytes);
    f.hint = LOW_RESERVED / PAGE_SIZE;
}

/// Allocates one zeroed frame and returns its physical address.
pub fn alloc() -> Option<u64> {
    alloc_contiguous(1, u64::MAX)
}

/// Allocates `count` physically contiguous zeroed frames below `limit`.
pub fn alloc_contiguous(count: u64, limit: u64) -> Option<u64> {
    let phys = {
        let mut f = FRAMES.lock();
        let max = f.frames.min(limit / PAGE_SIZE);
        let mut found = None;
        // Single frames: start at the hint; runs: first fit from the bottom.
        let mut start = if count == 1 { f.hint } else { LOW_RESERVED / PAGE_SIZE };
        let mut wrapped = count != 1;
        loop {
            if start + count > max {
                if wrapped {
                    break;
                }
                wrapped = true;
                start = LOW_RESERVED / PAGE_SIZE;
                continue;
            }
            // Skip whole used words quickly.
            if start % 64 == 0 && *f.word((start / 64) as usize) == u64::MAX {
                start += 64;
                continue;
            }
            match (0..count).find(|&i| f.is_used(start + i)) {
                None => {
                    found = Some(start);
                    break;
                }
                Some(i) => start += i + 1,
            }
        }
        let first = found?;
        for i in 0..count {
            f.set(first + i, true);
        }
        if count == 1 {
            f.hint = first + 1;
        }
        first * PAGE_SIZE
    };
    // SAFETY: the frames were just reserved and are reachable through the
    // direct map.
    unsafe { core::ptr::write_bytes(phys_to_virt(phys) as *mut u8, 0, (count * PAGE_SIZE) as usize) };
    Some(phys)
}

pub fn free(phys: u64) {
    free_contiguous(phys, 1);
}

pub fn free_contiguous(phys: u64, count: u64) {
    let mut f = FRAMES.lock();
    let first = phys / PAGE_SIZE;
    for i in 0..count {
        debug_assert!(f.is_used(first + i), "double free of frame {:#x}", (first + i) * PAGE_SIZE);
        f.set(first + i, false);
    }
    if first < f.hint {
        f.hint = first;
    }
}

/// (free, usable) bytes of physical memory.
pub fn stats() -> (u64, u64) {
    let f = FRAMES.lock();
    (f.free * PAGE_SIZE, f.total_usable * PAGE_SIZE)
}

/// Highest physical address of RAM.
pub fn top() -> u64 {
    FRAMES.lock().frames * PAGE_SIZE
}
