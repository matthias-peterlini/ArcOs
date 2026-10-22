//! Text on a linear 32-bit framebuffer, drawn with the 8x16 BIOS font.
//!
//! Drawing goes to a shadow buffer in RAM; only changed pixel rows are
//! copied to the (write-combined, slow to read) framebuffer.

use super::{Cell, Surface, PALETTE};
use alloc::vec;
use alloc::vec::Vec;

pub struct FbSurface {
    fb: *mut u8,
    width: usize,
    height: usize,
    pitch: usize,
    shift: (u8, u8, u8),
    font: [u8; 4096],
    shadow: Vec<u32>,
    cols: usize,
    rows: usize,
    x0: usize,
    y0: usize,
    dirty: (usize, usize),
}

// SAFETY: the framebuffer mapping is owned by the console.
unsafe impl Send for FbSurface {}

const CW: usize = 8;
const CH: usize = 16;

impl FbSurface {
    /// # Safety
    /// `fb` must map `pitch * height` bytes of framebuffer memory.
    pub unsafe fn new(fb: *mut u8, width: usize, height: usize, pitch: usize, shift: (u8, u8, u8), font: &[u8; 4096]) -> FbSurface {
        let cols = (width - 16) / CW;
        let rows = (height - 16) / CH;
        let mut s = FbSurface {
            fb,
            width,
            height,
            pitch,
            shift,
            font: *font,
            shadow: vec![0; width * height],
            cols,
            rows,
            x0: (width - cols * CW) / 2,
            y0: (height - rows * CH) / 2,
            dirty: (0, height),
        };
        let bg = s.rgb(PALETTE[super::DEFAULT_BG as usize]);
        s.shadow.fill(bg);
        s
    }

    fn rgb(&self, c: u32) -> u32 {
        let (r, g, b) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
        r << self.shift.0 | g << self.shift.1 | b << self.shift.2
    }

    fn mark(&mut self, y0: usize, y1: usize) {
        self.dirty = (self.dirty.0.min(y0), self.dirty.1.max(y1));
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// Fills a rectangle in pixels (for the boot splash and progress bars).
    pub fn fill_rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let c = self.rgb(color);
        for yy in y.min(self.height)..(y + h).min(self.height) {
            let row = yy * self.width;
            self.shadow[row + x.min(self.width)..row + (x + w).min(self.width)].fill(c);
        }
        self.mark(y, y + h);
    }
}

impl Surface for FbSurface {
    fn cols(&self) -> usize {
        self.cols
    }

    fn rows(&self) -> usize {
        self.rows
    }

    fn draw(&mut self, col: usize, row: usize, cell: Cell, cursor: bool) {
        let fg = self.rgb(PALETTE[cell.fg as usize]);
        let bg = self.rgb(PALETTE[cell.bg as usize]);
        let px = self.x0 + col * CW;
        let py = self.y0 + row * CH;
        let glyph = &self.font[cell.glyph as usize * CH..cell.glyph as usize * CH + CH];
        for (dy, &bits) in glyph.iter().enumerate() {
            let bits = if cursor && dy >= CH - 2 { 0xFF } else { bits };
            let line = (py + dy) * self.width + px;
            for dx in 0..CW {
                self.shadow[line + dx] = if bits & (0x80 >> dx) != 0 { fg } else { bg };
            }
        }
        self.mark(py, py + CH);
    }

    fn scroll_up(&mut self, bg: u8) {
        let line = self.width * CH;
        let top = self.y0 * self.width;
        let end = (self.y0 + self.rows * CH) * self.width;
        self.shadow.copy_within(top + line..end, top);
        let c = self.rgb(PALETTE[bg as usize]);
        self.shadow[end - line..end].fill(c);
        self.mark(self.y0, self.y0 + self.rows * CH);
    }

    fn clear(&mut self, bg: u8) {
        let c = self.rgb(PALETTE[bg as usize]);
        self.shadow.fill(c);
        self.mark(0, self.height);
    }

    fn flush(&mut self) {
        let (y0, y1) = self.dirty;
        if y0 >= y1 {
            return;
        }
        for y in y0..y1.min(self.height) {
            // SAFETY: row y of the framebuffer lies within the mapping.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    self.shadow.as_ptr().add(y * self.width),
                    self.fb.add(y * self.pitch) as *mut u32,
                    self.width,
                );
            }
        }
        self.dirty = (usize::MAX, 0);
    }
}
