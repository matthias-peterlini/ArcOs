//! The mouse pointer: drawn straight on the framebuffer over the back
//! buffer, so moving it costs a few hundred pixels and no redraw.

use super::gfx::{blend, Canvas, Rect};
use super::Display;
use crate::dev::input;

const W: i32 = 16;
const H: i32 = 24;

pub struct Cursor {
    pub x: i32,
    pub y: i32,
    pub buttons: u8,
    img: Canvas,
    drawn: Option<Rect>,
    raw: Option<(i32, i32)>,
    wheel: Option<i32>,
    /// Wheel movement since the last poll (positive: towards the user).
    pub scrolled: i32,
}

/// Is (`x`, `y`) (in 1/4 pixels) inside the polygon? Even-odd rule.
fn inside(poly: &[(i32, i32)], x: i32, y: i32) -> bool {
    let mut odd = false;
    let mut j = poly.len() - 1;
    for i in 0..poly.len() {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            odd = !odd;
        }
        j = i;
    }
    odd
}

/// The arrow, anti-aliased with 4x4 samples per pixel: white with a dark
/// outline.
fn arrow() -> Canvas {
    let shape: [(i32, i32); 7] = [(1, 1), (1, 18), (5, 14), (8, 21), (11, 20), (8, 13), (13, 13)];
    let q = |pts: &[(i32, i32)], grow: i32| -> alloc::vec::Vec<(i32, i32)> {
        // Grow the shape around its middle by `grow` quarter pixels.
        let (cx, cy) = (6 * 4, 11 * 4);
        pts.iter()
            .map(|&(x, y)| {
                let (x, y) = (x * 4, y * 4);
                (x + (x - cx).signum() * grow, y + (y - cy).signum() * grow)
            })
            .collect()
    };
    let outer = q(&shape, 5);
    let inner = q(&shape, 0);
    let mut c = Canvas::new(W, H, 0);
    for y in 0..H {
        for x in 0..W {
            let (mut o, mut i) = (0u32, 0u32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let (px, py) = (x * 4 + sx, y * 4 + sy);
                    o += inside(&outer, px, py) as u32;
                    i += inside(&inner, px, py) as u32;
                }
            }
            if o == 0 {
                continue;
            }
            // Dark edge, white inside.
            let a = o * 255 / 16;
            let white = i * 255 / 16;
            let v = 0x20 + (white * (0xFF - 0x20) / 255);
            c.px[(y * W + x) as usize] = (a << 24) | (v << 16) | (v << 8) | v;
        }
    }
    c
}

impl Cursor {
    pub fn new(w: i32, h: i32) -> Cursor {
        Cursor { x: w / 2, y: h / 2, buttons: 0, img: arrow(), drawn: None, raw: None, wheel: None, scrolled: 0 }
    }

    /// Reads the pointing device. Returns whether the position changed.
    pub fn poll(&mut self, w: i32, h: i32) -> bool {
        let Some(p) = input::pointer() else { return false };
        self.buttons = p.buttons;
        self.scrolled = self.wheel.map_or(0, |w| w - p.wheel);
        self.wheel = Some(p.wheel);
        let (old_x, old_y) = (self.x, self.y);
        if p.absolute {
            self.x = p.x;
            self.y = p.y;
        } else {
            if let Some((rx, ry)) = self.raw {
                // Drivers report motion in screen directions (down is
                // positive), whatever the device counts.
                self.x += p.x - rx;
                self.y += p.y - ry;
            }
            self.raw = Some((p.x, p.y));
        }
        self.x = self.x.clamp(0, w - 1);
        self.y = self.y.clamp(0, h - 1);
        (self.x, self.y) != (old_x, old_y)
    }

    /// Where a click reported at device position (`x`, `y`) happened on
    /// the screen.
    pub fn click_at(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32) {
        match input::pointer() {
            Some(p) if p.absolute => (x.clamp(0, w - 1), y.clamp(0, h - 1)),
            _ => (self.x, self.y),
        }
    }

    /// Puts the screen under the pointer back as the back buffer has it.
    pub fn hide(&mut self, d: &Display) {
        if let Some(r) = self.drawn.take() {
            d.present(r);
        }
    }

    pub fn show(&mut self, d: &Display) {
        let r = Rect::new(self.x, self.y, W, H).intersect(&d.scene.bounds());
        for y in r.y..r.bottom() {
            for x in r.x..r.right() {
                let p = self.img.get(x - self.x, y - self.y);
                let a = p >> 24;
                if a != 0 {
                    d.put_screen(x, y, blend(d.scene.get(x, y), p, a));
                }
            }
        }
        crate::dev::display::flush(r.x, r.y, r.w, r.h);
        self.drawn = Some(r);
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, W, H)
    }
}
