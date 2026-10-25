//! A decoder for QOI ("Quite OK Image") files, the format of the icons and
//! wallpapers in `sys:/system/gui` (written by `tools/assets/qoi.py`).

use super::gfx::Canvas;
use alloc::vec::Vec;

/// Decodes a QOI file into a canvas of `0xAARRGGBB` pixels.
pub fn decode(data: &[u8]) -> Option<Canvas> {
    if data.len() < 22 || &data[..4] != b"qoif" {
        return None;
    }
    let w = u32::from_be_bytes(data[4..8].try_into().ok()?);
    let h = u32::from_be_bytes(data[8..12].try_into().ok()?);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return None;
    }
    let total = (w * h) as usize;
    let mut px: Vec<u32> = Vec::with_capacity(total);
    let mut index = [0u32; 64];
    let (mut r, mut g, mut b, mut a) = (0u8, 0u8, 0u8, 255u8);
    let mut i = 14;
    let end = data.len() - 8;
    while px.len() < total && i < end {
        let op = data[i];
        i += 1;
        let mut run = 1;
        match op {
            0xFE => {
                r = data[i];
                g = data[i + 1];
                b = data[i + 2];
                i += 3;
            }
            0xFF => {
                r = data[i];
                g = data[i + 1];
                b = data[i + 2];
                a = data[i + 3];
                i += 4;
            }
            _ => match op >> 6 {
                0 => {
                    let c = index[op as usize];
                    a = (c >> 24) as u8;
                    r = (c >> 16) as u8;
                    g = (c >> 8) as u8;
                    b = c as u8;
                }
                1 => {
                    r = r.wrapping_add(((op >> 4) & 3).wrapping_sub(2));
                    g = g.wrapping_add(((op >> 2) & 3).wrapping_sub(2));
                    b = b.wrapping_add((op & 3).wrapping_sub(2));
                }
                2 => {
                    let dg = (op & 0x3F).wrapping_sub(32);
                    let next = data[i];
                    i += 1;
                    r = r.wrapping_add(dg.wrapping_add((next >> 4).wrapping_sub(8)));
                    g = g.wrapping_add(dg);
                    b = b.wrapping_add(dg.wrapping_add((next & 0xF).wrapping_sub(8)));
                }
                _ => run = (op & 0x3F) as usize + 1,
            },
        }
        let c = (a as u32) << 24 | (r as u32) << 16 | (g as u32) << 8 | b as u32;
        let hash = (r as usize * 3 + g as usize * 5 + b as usize * 7 + a as usize * 11) % 64;
        index[hash] = c;
        for _ in 0..run.min(total - px.len()) {
            px.push(c);
        }
    }
    px.resize(total, 0);
    Some(Canvas::from_pixels(w as i32, h as i32, px))
}
