//! Drawing: pixel buffers and the shapes the interface is made of.
//!
//! The kernel is built without floating point, so everything here is integer
//! arithmetic: alpha blending in 0..=255, anti-aliased rounded corners from an
//! integer square root at 1/16 pixel, box blurs with running sums.
//!
//! Colours are `0xAARRGGBB`. A [`Canvas`] used as a screen ignores alpha; one
//! holding an icon keeps it (straight, not premultiplied).

use alloc::vec;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub const EMPTY: Rect = Rect::new(0, 0, 0, 0);

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }

    pub fn intersect(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        if r <= x || b <= y {
            Rect::EMPTY
        } else {
            Rect::new(x, y, r - x, b - y)
        }
    }

    pub fn overlaps(&self, o: &Rect) -> bool {
        !self.intersect(o).is_empty()
    }

    pub fn union(&self, o: &Rect) -> Rect {
        if self.is_empty() {
            return *o;
        }
        if o.is_empty() {
            return *self;
        }
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect::new(x, y, self.right().max(o.right()) - x, self.bottom().max(o.bottom()) - y)
    }

    pub fn grow(&self, d: i32) -> Rect {
        Rect::new(self.x - d, self.y - d, self.w + 2 * d, self.h + 2 * d)
    }

    pub fn offset(&self, dx: i32, dy: i32) -> Rect {
        Rect::new(self.x + dx, self.y + dy, self.w, self.h)
    }
}

/// `dst` with `src` drawn over it at opacity `a` (0..=255).
#[inline]
pub fn blend(dst: u32, src: u32, a: u32) -> u32 {
    if a >= 255 {
        return src | 0xFF00_0000;
    }
    if a == 0 {
        return dst;
    }
    let inv = 255 - a;
    let mix = |s: u32, d: u32| {
        let x = s * a + d * inv + 128;
        (x + (x >> 8)) >> 8
    };
    let r = mix((src >> 16) & 0xFF, (dst >> 16) & 0xFF);
    let g = mix((src >> 8) & 0xFF, (dst >> 8) & 0xFF);
    let b = mix(src & 0xFF, dst & 0xFF);
    0xFF00_0000 | (r << 16) | (g << 8) | b
}

/// Linear mix of two colours, `t` in 0..=255.
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    let alpha = |c: u32| c >> 24;
    let ch = |x: u32, y: u32| (x * (255 - t) + y * t + 127) / 255;
    let out = blend(a, b, t) & 0xFF_FFFF;
    (ch(alpha(a), alpha(b)) << 24) | out
}

/// Integer square root.
pub fn isqrt(n: u32) -> u32 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

/// How much of the pixel at (`px`, `py`) a rounded rectangle covers, 0..=16.
#[inline]
fn round_cover(r: &Rect, rad: i32, px: i32, py: i32) -> u32 {
    if rad <= 0 {
        return 16;
    }
    let cx = if px < r.x + rad {
        r.x + rad
    } else if px >= r.right() - rad {
        r.right() - rad
    } else {
        return 16;
    };
    let cy = if py < r.y + rad {
        r.y + rad
    } else if py >= r.bottom() - rad {
        r.bottom() - rad
    } else {
        return 16;
    };
    let dx = px * 16 + 8 - cx * 16;
    let dy = py * 16 + 8 - cy * 16;
    let d = isqrt((dx * dx + dy * dy) as u32) as i32;
    (rad * 16 - d + 8).clamp(0, 16) as u32
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    clip: Rect,
    /// A layer keeps its alpha: drawing composites over what is there
    /// instead of treating it as opaque.
    layer: bool,
}

/// `src` at opacity `a` over `dst`, both with their own alpha (straight).
#[inline]
pub fn over(dst: u32, src: u32, a: u32) -> u32 {
    let da = dst >> 24;
    if da == 255 {
        return blend(dst, src, a);
    }
    if a == 0 {
        return dst;
    }
    // out = a + da * (1 - a)
    let keep = da * (255 - a) / 255;
    let oa = a + keep;
    if oa == 0 {
        return 0;
    }
    let ch = |s: u32, d: u32| (s * a + d * keep) / oa;
    let r = ch((src >> 16) & 0xFF, (dst >> 16) & 0xFF);
    let g = ch((src >> 8) & 0xFF, (dst >> 8) & 0xFF);
    let b = ch(src & 0xFF, dst & 0xFF);
    (oa << 24) | (r << 16) | (g << 8) | b
}

/// Integer angle of (`dx`, `dy`) clockwise from straight up, in 1/1000 of
/// a turn (screen coordinates: y grows downwards).
pub fn angle_permille(dx: i32, dy: i32) -> u32 {
    if dx == 0 && dy == 0 {
        return 0;
    }
    // atan(t) for t in 0..=1, in 1/1000 turn: t * (125 + 43 * (1 - t))
    // (within 0.3 degrees).
    let (ax, ay) = (dx.unsigned_abs() as i64, dy.unsigned_abs() as i64);
    let (small, big) = if ax < ay { (ax, ay) } else { (ay, ax) };
    let t = small * 1024 / big; // 0..=1024
    let mut a = (t * (125 * 1024 + 43 * (1024 - t)) / (1024 * 1024)) as i64; // 0..=125
    if ax > ay {
        a = 250 - a; // measured from the vertical axis
    }
    // a: 0..=250 from up towards the side, in the quadrant of (dx, dy).
    let a = match (dx >= 0, dy < 0) {
        (true, true) => a,
        (true, false) => 500 - a,
        (false, false) => 500 + a,
        (false, true) => 1000 - a,
    };
    (a as u32) % 1000
}

impl Canvas {
    pub fn new(w: i32, h: i32, fill: u32) -> Canvas {
        Canvas { w, h, px: vec![fill; (w.max(0) * h.max(0)) as usize], clip: Rect::new(0, 0, w, h), layer: false }
    }

    pub fn from_pixels(w: i32, h: i32, px: Vec<u32>) -> Canvas {
        Canvas { w, h, px, clip: Rect::new(0, 0, w, h), layer: false }
    }

    /// A transparent canvas that keeps alpha (a window drawn on its own).
    pub fn new_layer(w: i32, h: i32) -> Canvas {
        let mut c = Canvas::new(w, h, 0);
        c.layer = true;
        c
    }

    pub fn bounds(&self) -> Rect {
        Rect::new(0, 0, self.w, self.h)
    }

    pub fn set_clip(&mut self, r: Rect) {
        self.clip = r.intersect(&self.bounds());
    }

    pub fn clip(&self) -> Rect {
        self.clip
    }

    pub fn reset_clip(&mut self) {
        self.clip = self.bounds();
    }

    #[inline]
    pub fn get(&self, x: i32, y: i32) -> u32 {
        self.px[(y * self.w + x) as usize]
    }

    #[inline]
    fn put(&mut self, x: i32, y: i32, c: u32, a: u32) {
        let i = (y * self.w + x) as usize;
        self.px[i] = if self.layer { over(self.px[i], c, a) } else { blend(self.px[i], c, a) };
    }

    /// Fills a rectangle (blended by the colour's alpha).
    pub fn fill(&mut self, r: Rect, c: u32) {
        let a = c >> 24;
        let r = r.intersect(&self.clip);
        if r.is_empty() || a == 0 {
            return;
        }
        for y in r.y..r.bottom() {
            let row = (y * self.w) as usize;
            let line = &mut self.px[row + r.x as usize..row + r.right() as usize];
            if a == 255 {
                line.fill(c);
            } else if self.layer {
                for p in line {
                    *p = over(*p, c, a);
                }
            } else {
                for p in line {
                    *p = blend(*p, c, a);
                }
            }
        }
    }

    /// Fills a rounded rectangle with anti-aliased corners.
    pub fn fill_round(&mut self, r: Rect, rad: i32, c: u32) {
        self.fill_round_grad(r, rad, c, c);
    }

    /// A rounded rectangle with a vertical gradient from `top` to `bottom`.
    pub fn fill_round_grad(&mut self, r: Rect, rad: i32, top: u32, bottom: u32) {
        let rad = rad.min(r.w / 2).min(r.h / 2);
        let area = r.intersect(&self.clip);
        if area.is_empty() {
            return;
        }
        for y in area.y..area.bottom() {
            let t = if r.h > 1 { ((y - r.y) * 255 / (r.h - 1)) as u32 } else { 0 };
            let c = if top == bottom { top } else { mix(top, bottom, t) };
            let a = c >> 24;
            let corner_row = y < r.y + rad || y >= r.bottom() - rad;
            for x in area.x..area.right() {
                let cov = if corner_row { round_cover(&r, rad, x, y) } else { 16 };
                if cov == 0 {
                    continue;
                }
                self.put(x, y, c, a * cov / 16);
            }
        }
    }

    /// A soft radial glow of `c` around (`cx`, `cy`), fading out at `radius`;
    /// `strength` is the opacity at the centre (0..=255).
    pub fn glow(&mut self, cx: i32, cy: i32, radius: i32, c: u32, strength: u32) {
        let r = Rect::new(cx - radius, cy - radius, radius * 2, radius * 2);
        self.glow_round(r, 0, cx, cy, radius, c, strength);
    }

    /// [`glow`](Self::glow), kept inside the rounded rectangle `r` (the rust
    /// glow in the corner of a ticly card).
    #[allow(clippy::too_many_arguments)]
    pub fn glow_round(&mut self, r: Rect, rad: i32, cx: i32, cy: i32, radius: i32, c: u32, strength: u32) {
        let radius = radius.max(1);
        let rad = rad.min(r.w / 2).min(r.h / 2);
        let area = r.intersect(&Rect::new(cx - radius, cy - radius, radius * 2, radius * 2)).intersect(&self.clip);
        let r2 = radius as i64 * radius as i64;
        for y in area.y..area.bottom() {
            let dy = (y - cy) as i64;
            for x in area.x..area.right() {
                let dx = (x - cx) as i64;
                let d2 = dx * dx + dy * dy;
                if d2 >= r2 {
                    continue;
                }
                // (1 - d/r)^2, smooth at the centre and at the edge.
                let f = 1024 - (isqrt((d2 * 1024 * 1024 / r2) as u32) as i64);
                let a = (strength as i64 * f * f / (1024 * 1024)) as u32;
                let cov = round_cover(&r, rad, x, y);
                if a > 0 && cov > 0 {
                    self.put(x, y, c, a * cov / 16);
                }
            }
        }
    }

    /// The outline of a rounded rectangle, `width` pixels wide.
    pub fn stroke_round(&mut self, r: Rect, rad: i32, width: i32, c: u32) {
        let inner = r.grow(-width);
        let rad = rad.min(r.w / 2).min(r.h / 2);
        let irad = (rad - width).max(0);
        let area = r.intersect(&self.clip);
        let a = c >> 24;
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let outer = round_cover(&r, rad, x, y);
                let inn = if inner.contains(x, y) { round_cover(&inner, irad, x, y) } else { 0 };
                let cov = outer.saturating_sub(inn);
                if cov > 0 {
                    self.put(x, y, c, a * cov / 16);
                }
            }
        }
    }

    /// A filled anti-aliased circle.
    pub fn fill_circle(&mut self, cx: i32, cy: i32, rad: i32, c: u32) {
        self.fill_round(Rect::new(cx - rad, cy - rad, rad * 2, rad * 2), rad, c);
    }

    /// Copies part of an opaque canvas (`src` at `sx`, `sy`) to `dst`.
    pub fn blit(&mut self, src: &Canvas, sx: i32, sy: i32, dst: Rect) {
        let d = dst.intersect(&self.clip);
        if d.is_empty() {
            return;
        }
        let (ox, oy) = (sx + d.x - dst.x, sy + d.y - dst.y);
        for y in 0..d.h {
            let syy = oy + y;
            if syy < 0 || syy >= src.h {
                continue;
            }
            let x0 = ox.max(0);
            let x1 = (ox + d.w).min(src.w);
            if x1 <= x0 {
                continue;
            }
            let s = (syy * src.w + x0) as usize;
            let t = ((d.y + y) * self.w + d.x + (x0 - ox)) as usize;
            let n = (x1 - x0) as usize;
            self.px[t..t + n].copy_from_slice(&src.px[s..s + n]);
        }
    }

    /// Like [`blit`](Self::blit), but only inside a rounded rectangle and
    /// with a tint drawn over it: frosted glass, when `src` is the blurred
    /// wallpaper (read at the same screen position).
    pub fn blit_round(&mut self, src: &Canvas, r: Rect, rad: i32, tint: u32) {
        self.blit_round_from(src, r.x, r.y, r, rad, tint);
    }

    /// Copies `src` from (`sx`, `sy`) into the rounded rectangle `r`.
    pub fn blit_round_from(&mut self, src: &Canvas, sx: i32, sy: i32, r: Rect, rad: i32, tint: u32) {
        let rad = rad.min(r.w / 2).min(r.h / 2);
        let area = r.intersect(&self.clip);
        let ta = tint >> 24;
        for y in area.y..area.bottom() {
            let syy = y - r.y + sy;
            if syy < 0 || syy >= src.h {
                continue;
            }
            let corner_row = y < r.y + rad || y >= r.bottom() - rad;
            for x in area.x..area.right() {
                let sxx = x - r.x + sx;
                if sxx < 0 || sxx >= src.w {
                    continue;
                }
                let cov = if corner_row { round_cover(&r, rad, x, y) } else { 16 };
                if cov == 0 {
                    continue;
                }
                let s = blend(src.get(sxx, syy), tint, ta);
                self.put(x, y, s, 255 * cov / 16);
            }
        }
    }

    /// Draws an image with its own alpha at (`x`, `y`), faded by `opacity`.
    pub fn draw_image(&mut self, img: &Canvas, x: i32, y: i32, opacity: u32) {
        let d = Rect::new(x, y, img.w, img.h).intersect(&self.clip);
        for yy in d.y..d.bottom() {
            for xx in d.x..d.right() {
                let s = img.get(xx - x, yy - y);
                let a = (s >> 24) * opacity / 255;
                if a != 0 {
                    self.put(xx, yy, s, a);
                }
            }
        }
    }

    /// Draws an alpha mask (one byte per pixel) in colour `c`.
    pub fn draw_mask(&mut self, mask: &[u8], mw: i32, mh: i32, x: i32, y: i32, c: u32) {
        let a = c >> 24;
        let d = Rect::new(x, y, mw, mh).intersect(&self.clip);
        for yy in d.y..d.bottom() {
            let row = ((yy - y) * mw) as usize;
            for xx in d.x..d.right() {
                let m = mask[row + (xx - x) as usize] as u32;
                if m != 0 {
                    self.put(xx, yy, c, a * m / 255);
                }
            }
        }
    }

    /// A soft shadow around `r` (outside it only), `size` pixels wide.
    pub fn shadow(&mut self, r: Rect, rad: i32, size: i32, strength: u32) {
        let rad = rad.min(r.w / 2).min(r.h / 2).max(0);
        let outer = r.grow(size).offset(0, size / 3);
        let area = outer.intersect(&self.clip);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                if r.contains(x, y) && round_cover(&r, rad, x, y) == 16 {
                    continue;
                }
                // Distance outside the (offset) rounded rectangle, in pixels.
                let rr = r.offset(0, size / 3);
                let cx = x.clamp(rr.x + rad, (rr.right() - rad - 1).max(rr.x + rad));
                let cy = y.clamp(rr.y + rad, (rr.bottom() - rad - 1).max(rr.y + rad));
                let (dx, dy) = (x - cx, y - cy);
                let d = isqrt((dx * dx + dy * dy) as u32) as i32 - rad;
                if d >= size {
                    continue;
                }
                let t = (size - d.max(0)) as u32;
                let a = strength * t * t / (size as u32 * size as u32);
                if a > 0 {
                    self.put(x, y, 0, a);
                }
            }
        }
    }

    /// Blurs the whole canvas (three box passes ≈ a Gaussian of `radius`).
    pub fn blur(&mut self, radius: i32) {
        for _ in 0..3 {
            box_blur_h(self, radius);
            box_blur_v(self, radius);
        }
    }

    /// A copy scaled to `w` x `h`: area averaging when shrinking, bilinear
    /// when growing. Alpha is kept and colours are weighted by it.
    pub fn scaled(&self, w: i32, h: i32) -> Canvas {
        let mut out = Canvas::new(w, h, 0);
        if w <= 0 || h <= 0 || self.w <= 0 || self.h <= 0 {
            return out;
        }
        if w <= self.w && h <= self.h {
            // Area average, in 1/256 of a source pixel.
            for y in 0..h {
                let y0 = (y as i64 * self.h as i64 * 256 / h as i64) as i32;
                let y1 = ((y + 1) as i64 * self.h as i64 * 256 / h as i64) as i32;
                for x in 0..w {
                    let x0 = (x as i64 * self.w as i64 * 256 / w as i64) as i32;
                    let x1 = ((x + 1) as i64 * self.w as i64 * 256 / w as i64) as i32;
                    let (mut sa, mut sr, mut sg, mut sb, mut wt) = (0u64, 0u64, 0u64, 0u64, 0u64);
                    let mut sy = y0 >> 8;
                    while (sy << 8) < y1 && sy < self.h {
                        let wy = ((sy + 1) * 256).min(y1) - (sy * 256).max(y0);
                        let mut sx = x0 >> 8;
                        while (sx << 8) < x1 && sx < self.w {
                            let wx = ((sx + 1) * 256).min(x1) - (sx * 256).max(x0);
                            let k = (wx * wy) as u64;
                            let p = self.get(sx, sy);
                            let a = (p >> 24) as u64;
                            sa += a * k;
                            sr += ((p >> 16) & 0xFF) as u64 * a * k;
                            sg += ((p >> 8) & 0xFF) as u64 * a * k;
                            sb += (p & 0xFF) as u64 * a * k;
                            wt += k;
                            sx += 1;
                        }
                        sy += 1;
                    }
                    if wt == 0 || sa == 0 {
                        continue;
                    }
                    let a = sa / wt;
                    let (r, g, b) = (sr / sa, sg / sa, sb / sa);
                    out.px[(y * w + x) as usize] = ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                }
            }
        } else {
            for y in 0..h {
                let fy = (y as i64 * 2 + 1) * self.h as i64 * 128 / h as i64 - 128;
                let (y0, ty) = ((fy >> 8).clamp(0, self.h as i64 - 1) as i32, (fy & 0xFF) as u32);
                let y1 = (y0 + 1).min(self.h - 1);
                for x in 0..w {
                    let fx = (x as i64 * 2 + 1) * self.w as i64 * 128 / w as i64 - 128;
                    let (x0, tx) = ((fx >> 8).clamp(0, self.w as i64 - 1) as i32, (fx & 0xFF) as u32);
                    let x1 = (x0 + 1).min(self.w - 1);
                    let top = mix(self.get(x0, y0), self.get(x1, y0), tx);
                    let bot = mix(self.get(x0, y1), self.get(x1, y1), tx);
                    out.px[(y * w + x) as usize] = mix(top, bot, ty);
                }
            }
        }
        out
    }

    /// Draws `img` stretched into `dst` (nearest pixel, fast enough to
    /// animate), with its alpha and faded by `opacity`.
    pub fn draw_scaled(&mut self, img: &Canvas, dst: Rect, opacity: u32) {
        if dst.is_empty() || img.w <= 0 || img.h <= 0 || opacity == 0 {
            return;
        }
        let d = dst.intersect(&self.clip);
        if d.is_empty() {
            return;
        }
        let sx_step = ((img.w as i64) << 16) / dst.w as i64;
        let sy_step = ((img.h as i64) << 16) / dst.h as i64;
        for y in d.y..d.bottom() {
            let sy = (((y - dst.y) as i64 * sy_step) >> 16).min(img.h as i64 - 1) as i32;
            let srow = (sy * img.w) as usize;
            let row = (y * self.w) as usize;
            let mut fx = (d.x - dst.x) as i64 * sx_step;
            for x in d.x..d.right() {
                let sx = ((fx >> 16) as i32).min(img.w - 1);
                fx += sx_step;
                let s = img.px[srow + sx as usize];
                let a = (s >> 24) * opacity / 255;
                if a != 0 {
                    let i = row + x as usize;
                    self.px[i] = if self.layer { over(self.px[i], s, a) } else { blend(self.px[i], s, a) };
                }
            }
        }
    }

    /// Copies part of an opaque canvas like [`blit`](Self::blit), at
    /// `opacity` over what is there.
    pub fn blit_faded(&mut self, src: &Canvas, sx: i32, sy: i32, dst: Rect, opacity: u32) {
        if opacity >= 255 {
            return self.blit(src, sx, sy, dst);
        }
        let d = dst.intersect(&self.clip);
        for y in d.y..d.bottom() {
            let syy = sy + y - dst.y;
            if syy < 0 || syy >= src.h {
                continue;
            }
            for x in d.x..d.right() {
                let sxx = sx + x - dst.x;
                if sxx >= 0 && sxx < src.w {
                    let i = (y * self.w + x) as usize;
                    self.px[i] = blend(self.px[i], src.get(sxx, syy), opacity);
                }
            }
        }
    }

    /// A line `width` pixels wide with round ends, anti-aliased.
    pub fn line(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, width: i32, c: u32) {
        let half = (width.max(1) * 8) as i64; // in 1/16 px
        let box_ = Rect::new(x0.min(x1) - width, y0.min(y1) - width, (x0 - x1).abs() + 2 * width + 1, (y0 - y1).abs() + 2 * width + 1);
        let area = box_.intersect(&self.clip);
        let (ax, ay, bx, by) = (x0 as i64 * 16 + 8, y0 as i64 * 16 + 8, x1 as i64 * 16 + 8, y1 as i64 * 16 + 8);
        let (vx, vy) = (bx - ax, by - ay);
        let len2 = vx * vx + vy * vy;
        let a = c >> 24;
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let (px, py) = (x as i64 * 16 + 8, y as i64 * 16 + 8);
                let t = if len2 == 0 { 0 } else { (((px - ax) * vx + (py - ay) * vy) * 1024 / len2).clamp(0, 1024) };
                let (qx, qy) = (ax + vx * t / 1024, ay + vy * t / 1024);
                let (dx, dy) = (px - qx, py - qy);
                let d = isqrt((dx * dx + dy * dy).min(u32::MAX as i64) as u32) as i64;
                let cov = (half - d + 8).clamp(0, 16) as u32;
                if cov > 0 {
                    self.put(x, y, c, a * cov / 16);
                }
            }
        }
    }

    /// Part of a ring (centre, outer radius, thickness): from straight up,
    /// clockwise, `permille` of a full turn.
    pub fn arc(&mut self, cx: i32, cy: i32, rad: i32, thick: i32, permille: u32, c: u32) {
        let area = Rect::new(cx - rad - 1, cy - rad - 1, 2 * rad + 2, 2 * rad + 2).intersect(&self.clip);
        let a = c >> 24;
        let (ro, ri) = (rad * 16, (rad - thick) * 16);
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let (dx, dy) = ((x - cx) * 16 + 8, (y - cy) * 16 + 8);
                let d = isqrt((dx * dx + dy * dy) as u32) as i32;
                let cov = ((ro - d + 8).clamp(0, 16)).min((d - ri + 8).clamp(0, 16)) as u32;
                if cov == 0 || (permille < 1000 && angle_permille(dx, dy) >= permille) {
                    continue;
                }
                self.put(x, y, c, a * cov / 16);
            }
        }
    }

    /// A filled triangle (no anti-aliasing; for small symbols).
    pub fn triangle(&mut self, p: [(i32, i32); 3], c: u32) {
        let (x0, x1) = (p.iter().map(|q| q.0).min().unwrap_or(0), p.iter().map(|q| q.0).max().unwrap_or(0));
        let (y0, y1) = (p.iter().map(|q| q.1).min().unwrap_or(0), p.iter().map(|q| q.1).max().unwrap_or(0));
        let area = Rect::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1).intersect(&self.clip);
        let edge = |a: (i32, i32), b: (i32, i32), x: i32, y: i32| (b.0 - a.0) as i64 * (y - a.1) as i64 - (b.1 - a.1) as i64 * (x - a.0) as i64;
        let a = c >> 24;
        // Four samples per pixel for soft edges.
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                let mut n = 0;
                for (sx, sy) in [(1, 1), (3, 1), (1, 3), (3, 3)] {
                    let (qx, qy) = (x * 4 + sx, y * 4 + sy);
                    let s = |a: (i32, i32)| (a.0 * 4 + 2, a.1 * 4 + 2);
                    let (e0, e1, e2) = (edge(s(p[0]), s(p[1]), qx, qy), edge(s(p[1]), s(p[2]), qx, qy), edge(s(p[2]), s(p[0]), qx, qy));
                    if (e0 >= 0 && e1 >= 0 && e2 >= 0) || (e0 <= 0 && e1 <= 0 && e2 <= 0) {
                        n += 1;
                    }
                }
                if n > 0 {
                    self.put(x, y, c, a * n / 4);
                }
            }
        }
    }

    /// A copy of a region.
    pub fn crop(&self, r: Rect) -> Canvas {
        let mut out = Canvas::new(r.w, r.h, 0);
        out.blit(self, r.x, r.y, out.bounds());
        out
    }
}

fn box_blur_h(c: &mut Canvas, rad: i32) {
    let w = c.w as usize;
    let n = (2 * rad + 1) as u32;
    let mut line = vec![0u32; w];
    for y in 0..c.h as usize {
        let row = &mut c.px[y * w..y * w + w];
        line.copy_from_slice(row);
        let at = |i: i32| line[i.clamp(0, w as i32 - 1) as usize];
        let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
        for i in -rad..=rad {
            let p = at(i);
            r += (p >> 16) & 0xFF;
            g += (p >> 8) & 0xFF;
            b += p & 0xFF;
        }
        for (x, out) in row.iter_mut().enumerate() {
            *out = 0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n);
            let (add, sub) = (at(x as i32 + rad + 1), at(x as i32 - rad));
            r = r + ((add >> 16) & 0xFF) - ((sub >> 16) & 0xFF);
            g = g + ((add >> 8) & 0xFF) - ((sub >> 8) & 0xFF);
            b = b + (add & 0xFF) - (sub & 0xFF);
        }
    }
}

fn box_blur_v(c: &mut Canvas, rad: i32) {
    let (w, h) = (c.w as usize, c.h as usize);
    let n = (2 * rad + 1) as u32;
    let mut col = vec![0u32; h];
    for x in 0..w {
        for (y, v) in col.iter_mut().enumerate() {
            *v = c.px[y * w + x];
        }
        let at = |i: i32| col[i.clamp(0, h as i32 - 1) as usize];
        let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
        for i in -rad..=rad {
            let p = at(i);
            r += (p >> 16) & 0xFF;
            g += (p >> 8) & 0xFF;
            b += p & 0xFF;
        }
        for y in 0..h {
            c.px[y * w + x] = 0xFF00_0000 | ((r / n) << 16) | ((g / n) << 8) | (b / n);
            let (add, sub) = (at(y as i32 + rad + 1), at(y as i32 - rad));
            r = r + ((add >> 16) & 0xFF) - ((sub >> 16) & 0xFF);
            g = g + ((add >> 8) & 0xFF) - ((sub >> 8) & 0xFF);
            b = b + (add & 0xFF) - (sub & 0xFF);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_go_round_clockwise() {
        assert_eq!(angle_permille(0, -10), 0);
        assert!((240..=260).contains(&angle_permille(10, 0)));
        assert!((490..=510).contains(&angle_permille(0, 10)));
        assert!((740..=760).contains(&angle_permille(-10, 0)));
        assert!((120..=130).contains(&angle_permille(10, -10)));
    }

    #[test]
    fn over_a_transparent_layer_keeps_the_colour() {
        assert_eq!(over(0, 0xFF12_3456, 255), 0xFF12_3456);
        let half = over(0, 0xFF12_3456, 128);
        assert_eq!(half >> 24, 128);
        assert_eq!(half & 0xFF_FFFF, 0x12_3456);
    }

    #[test]
    fn layers_keep_alpha_and_screens_do_not() {
        let mut layer = Canvas::new_layer(4, 4);
        layer.fill(Rect::new(0, 0, 4, 4), 0x80FF_FFFF);
        assert_eq!(layer.get(1, 1) >> 24, 0x80);
        let mut screen = Canvas::new(4, 4, 0xFF00_0000);
        screen.fill(Rect::new(0, 0, 4, 4), 0x80FF_FFFF);
        assert_eq!(screen.get(1, 1) >> 24, 0xFF);
    }
}
