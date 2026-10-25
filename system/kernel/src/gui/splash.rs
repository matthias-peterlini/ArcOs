//! The boot splash: the arc logo, the name and a progress bar. No text.

use super::gfx::{isqrt, Canvas, Rect};
use super::text::{self, Font};

const BG_TOP: u32 = 0xFF1A_1428;
const BG_BOTTOM: u32 = 0xFF2C_1F3F;

/// The arc logo: a thick half ring open at the top, with round ends, `r`
/// its outer radius.
pub fn draw_logo(c: &mut Canvas, cx: i32, cy: i32, r: i32, color: u32) {
    let t = (r * 2 / 5).max(3); // thickness
    let inner = r - t;
    let a = color >> 24;
    let box_ = Rect::new(cx - r - 1, cy - t / 2 - 1, 2 * r + 2, r + t / 2 + 2).intersect(&c.clip());
    let cap = |x: i32, y: i32, ex: i32| {
        let dx = x * 16 + 8 - ex * 16;
        let dy = y * 16 + 8 - cy * 16;
        let d = isqrt((dx * dx + dy * dy) as u32) as i32;
        (t * 8 - d + 8).clamp(0, 16)
    };
    for y in box_.y..box_.bottom() {
        for x in box_.x..box_.right() {
            let dx = x * 16 + 8 - cx * 16;
            let dy = y * 16 + 8 - cy * 16;
            let d = isqrt((dx * dx + dy * dy) as u32) as i32;
            let ring = (r * 16 - d + 8).clamp(0, 16).min((d - inner * 16 + 8).clamp(0, 16));
            let ring = if y * 16 + 8 >= cy * 16 { ring } else { 0 };
            let ends = cap(x, y, cx - r + t / 2).max(cap(x, y, cx + r - t / 2));
            let cov = ring.max(ends) as u32;
            if cov > 0 {
                c.fill(Rect::new(x, y, 1, 1), ((a * cov / 16) << 24) | (color & 0xFF_FFFF));
            }
        }
    }
}

/// The splash with the bar at `percent`; the first call draws everything,
/// later ones only the bar.
pub fn show(percent: u32) {
    super::with_display(|d| {
        let (w, h) = (d.width(), d.height());
        let cx = w / 2;
        let logo_y = h / 2 - 50;
        let bar = Rect::new(cx - 110, h / 2 + 70, 220, 6);
        let first = d.scene.get(0, 0) & 0xFF_FFFF == 0;
        let c = &mut d.scene;
        c.reset_clip();
        if first || percent == 0 {
            c.fill_round_grad(c.bounds(), 0, BG_TOP, BG_BOTTOM);
            draw_logo(c, cx, logo_y, 46, 0xFFFF_FFFF);
            text::draw_centered(c, Font::Display, cx, logo_y + 62, "arcOS", 0xFFF2_EFF7);
        }
        // The bar: a faint track and a white fill.
        let behind = Rect::new(bar.x - 2, bar.y - 2, bar.w + 4, bar.h + 4);
        c.fill_round_grad(behind, 0, sample(h, behind.y), sample(h, behind.bottom()));
        c.fill_round(bar, 3, 0x40FF_FFFF);
        let filled = bar.w * percent.min(100) as i32 / 100;
        if filled > 0 {
            c.fill_round(Rect::new(bar.x, bar.y, filled.max(6), bar.h), 3, 0xFFFF_FFFF);
        }
        if first || percent == 0 {
            d.present(d.scene.bounds());
        } else {
            d.present(behind);
        }
    });
}

/// The background colour at height `y` (to repaint behind the bar).
fn sample(h: i32, y: i32) -> u32 {
    super::gfx::mix(BG_TOP, BG_BOTTOM, ((y.clamp(0, h - 1)) * 255 / (h - 1).max(1)) as u32)
}
