//! 80x25 VGA text mode, used when the loader found no framebuffer mode.

use super::{Cell, Surface};
use crate::arch::cpu::outb;

pub struct VgaSurface {
    mem: *mut u16,
}

// SAFETY: the text buffer is owned by the console.
unsafe impl Send for VgaSurface {}

/// ANSI colour index -> VGA attribute colour.
const VGA: [u8; 18] = [0, 4, 2, 6, 1, 5, 3, 7, 8, 12, 10, 14, 9, 13, 11, 15, 15, 0];

impl VgaSurface {
    /// # Safety
    /// `mem` must map the 4000-byte VGA text buffer.
    pub unsafe fn new(mem: *mut u16) -> VgaSurface {
        outb(0x3D4, 0x0A); // hide the hardware cursor; we draw our own
        outb(0x3D5, 0x20);
        VgaSurface { mem }
    }

    fn put(&mut self, col: usize, row: usize, v: u16) {
        // SAFETY: col < 80, row < 25.
        unsafe { self.mem.add(row * 80 + col).write_volatile(v) };
    }
}

impl Surface for VgaSurface {
    fn cols(&self) -> usize {
        80
    }

    fn rows(&self) -> usize {
        25
    }

    fn draw(&mut self, col: usize, row: usize, cell: Cell, cursor: bool) {
        let (mut fg, mut bg) = (VGA[cell.fg as usize], VGA[cell.bg as usize]);
        if cursor {
            core::mem::swap(&mut fg, &mut bg);
        }
        self.put(col, row, (bg as u16) << 12 | (fg as u16) << 8 | cell.glyph as u16);
    }

    fn scroll_up(&mut self, bg: u8) {
        // SAFETY: inside the 80x25 buffer.
        unsafe { core::ptr::copy(self.mem.add(80), self.mem, 80 * 24) };
        for col in 0..80 {
            self.put(col, 24, (VGA[bg as usize] as u16) << 12 | b' ' as u16);
        }
    }

    fn clear(&mut self, bg: u8) {
        for i in 0..80 * 25 {
            self.put(i % 80, i / 80, (VGA[bg as usize] as u16) << 12 | b' ' as u16);
        }
    }

    fn flush(&mut self) {}
}
