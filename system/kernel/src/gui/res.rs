//! Pictures: the icon atlas and the wallpapers in `sys:/system/gui`.
//!
//! Icons are stored once at 128 px; each size the interface asks for is
//! scaled on first use and kept.

use super::gfx::{Canvas, Rect};
use super::qoi;
use crate::fs;
use crate::sync::IrqLock;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

const DIR: &str = "sys:/system/gui";
const CELL: i32 = 128;

struct Icons {
    atlas: Canvas,
    names: Vec<String>,
    cache: BTreeMap<(usize, i32), Canvas>,
}

static ICONS: IrqLock<Option<Icons>> = IrqLock::new(None);

fn read(path: &str) -> Option<Vec<u8>> {
    fs::abs(path).and_then(|p| fs::read_file(&p)).ok()
}

/// Loads the icons; `progress` gets 0..=100.
pub fn load(progress: impl Fn(u32)) {
    progress(0);
    let atlas = read(&alloc::format!("{DIR}/icons.qoi")).and_then(|d| qoi::decode(&d));
    progress(60);
    let names = read(&alloc::format!("{DIR}/icons.idx"))
        .map(|b| String::from_utf8_lossy(&b).lines().map(String::from).collect())
        .unwrap_or_default();
    match atlas {
        Some(atlas) => *ICONS.lock() = Some(Icons { atlas, names, cache: BTreeMap::new() }),
        None => crate::warn!("gui", "no icons in {DIR}"),
    }
    // The common sizes now, so the first frames are quick.
    for name in ["files", "terminal", "settings", "info", "apps", "trash", "text-editor", "store"] {
        with_icon(name, 52, |_| {});
    }
    progress(100);
}

/// Runs `f` with icon `name` at `size` pixels (if there is such an icon).
pub fn with_icon<T>(name: &str, size: i32, f: impl FnOnce(&Canvas) -> T) -> Option<T> {
    let mut guard = ICONS.lock();
    let icons = guard.as_mut()?;
    let id = icons.names.iter().position(|n| n == name)?;
    if !icons.cache.contains_key(&(id, size)) {
        let cols = icons.atlas.w / CELL;
        let cell = Rect::new((id as i32 % cols) * CELL, (id as i32 / cols) * CELL, CELL, CELL);
        let img = icons.atlas.crop(cell);
        let img = if size == CELL { img } else { img.scaled(size, size) };
        icons.cache.insert((id, size), img);
    }
    Some(f(&icons.cache[&(id, size)]))
}

/// Draws an icon; a plain rounded square if it is missing.
pub fn draw_icon(c: &mut Canvas, name: &str, size: i32, x: i32, y: i32) {
    draw_icon_faded(c, name, size, x, y, 255);
}

pub fn draw_icon_faded(c: &mut Canvas, name: &str, size: i32, x: i32, y: i32, opacity: u32) {
    if with_icon(name, size, |img| c.draw_image(img, x, y, opacity)).is_none() {
        c.fill_round(Rect::new(x + size / 10, y + size / 10, size * 8 / 10, size * 8 / 10), size / 5, 0xC0A0_A8B8);
    }
}

/// A wallpaper scaled to cover `w` x `h`, and a blurred copy of it for the
/// frosted-glass panels. A gradient if the file is missing.
pub fn wallpaper(name: &str, w: i32, h: i32) -> (Canvas, Canvas) {
    let img = read(&alloc::format!("{DIR}/wallpapers/{name}.qoi")).and_then(|d| qoi::decode(&d));
    let sharp = match img {
        Some(img) => {
            // Cover: scale so both sides fill the screen, then crop the middle.
            let (sw, sh) = if (img.w as i64 * h as i64) > (img.h as i64 * w as i64) {
                ((img.w as i64 * h as i64 / img.h as i64) as i32, h)
            } else {
                (w, (img.h as i64 * w as i64 / img.w as i64) as i32)
            };
            let scaled = img.scaled(sw, sh);
            scaled.crop(Rect::new((sw - w) / 2, (sh - h) / 2, w, h))
        }
        None => {
            let mut c = Canvas::new(w, h, 0xFF2A_2038);
            c.fill_round_grad(c.bounds(), 0, 0xFF3B_2E5A, 0xFFE0_8F6E);
            c
        }
    };
    let mut sharp = sharp;
    for p in sharp.px.iter_mut() {
        *p |= 0xFF00_0000;
    }
    // The blur is cheaper on a quarter-size copy, scaled back up.
    let mut soft = sharp.scaled(w / 4, h / 4);
    soft.blur(6);
    let soft = soft.scaled(w, h);
    (sharp, soft)
}

/// A small preview of a wallpaper (for the settings and the setup).
pub fn wallpaper_thumb(name: &str, w: i32, h: i32) -> Canvas {
    let img = read(&alloc::format!("{DIR}/wallpapers/{name}.qoi")).and_then(|d| qoi::decode(&d));
    match img {
        Some(img) => img.scaled(w, h),
        None => Canvas::new(w, h, 0xFF55_4466),
    }
}

pub const WALLPAPERS: [(&str, &str); 3] = [("dolomiti", "Dolomiti"), ("alba", "Alba"), ("notte", "Notte")];
