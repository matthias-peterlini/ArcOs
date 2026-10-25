//! Text, drawn from the anti-aliased glyph atlas in `assets/fonts.bin`
//! (rendered at build time by `tools/assets/gen_fonts.py`).

use super::gfx::Canvas;
use alloc::vec::Vec;

static ATLAS: &[u8] = include_bytes!("../../assets/fonts.bin");

/// The fonts in the atlas, in its order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Font {
    Body = 0,
    Strong = 1,
    Small = 2,
    Heading = 3,
    Title = 4,
    Clock = 5,
    Mono = 6,
    Display = 7,
}

#[derive(Clone, Copy)]
struct Glyph {
    code: u32,
    w: u8,
    h: u8,
    x: i8,
    y: i8,
    advance: u8,
    data: u32,
}

struct Face {
    line: u16,
    ascent: i16,
    glyphs: Vec<Glyph>,
}

struct Atlas {
    faces: Vec<Face>,
    data: usize,
}

static FACES: crate::sync::IrqLock<Option<Atlas>> = crate::sync::IrqLock::new(None);

fn u16_at(o: usize) -> u16 {
    u16::from_le_bytes([ATLAS[o], ATLAS[o + 1]])
}

fn u32_at(o: usize) -> u32 {
    u32::from_le_bytes([ATLAS[o], ATLAS[o + 1], ATLAS[o + 2], ATLAS[o + 3]])
}

fn parse() -> Atlas {
    assert!(&ATLAS[..4] == b"ARCF", "bad font atlas");
    let count = u16_at(6) as usize;
    let mut at = 8;
    let mut faces = Vec::new();
    for _ in 0..count {
        let line = u16_at(at + 2);
        let ascent = u16_at(at + 4) as i16;
        let n = u16_at(at + 6) as usize;
        at += 8;
        let mut glyphs = Vec::with_capacity(n);
        for _ in 0..n {
            glyphs.push(Glyph {
                code: u32_at(at),
                w: ATLAS[at + 4],
                h: ATLAS[at + 5],
                x: ATLAS[at + 6] as i8,
                y: ATLAS[at + 7] as i8,
                advance: ATLAS[at + 8],
                data: u32_at(at + 10),
            });
            at += 14;
        }
        glyphs.sort_by_key(|g| g.code);
        faces.push(Face { line, ascent, glyphs });
    }
    Atlas { faces, data: at }
}

fn with<T>(font: Font, f: impl FnOnce(&Face, usize) -> T) -> T {
    let mut guard = FACES.lock();
    let atlas = guard.get_or_insert_with(parse);
    f(&atlas.faces[font as usize], atlas.data)
}

fn find(face: &Face, c: char) -> Option<Glyph> {
    let c = match c {
        '\u{a0}' => ' ',
        c => c,
    };
    face.glyphs.binary_search_by_key(&(c as u32), |g| g.code).ok().map(|i| face.glyphs[i]).or_else(|| {
        // A missing letter: try without its accent, else a question mark.
        let plain = match c {
            'á' | 'â' | 'ä' => 'a',
            'ê' | 'ë' => 'e',
            'í' | 'î' | 'ï' => 'i',
            'ó' | 'ô' | 'ö' => 'o',
            'ú' | 'û' | 'ü' => 'u',
            _ => '?',
        };
        face.glyphs.binary_search_by_key(&(plain as u32), |g| g.code).ok().map(|i| face.glyphs[i])
    })
}

/// Width of `s` in pixels.
pub fn width(font: Font, s: &str) -> i32 {
    with(font, |face, _| s.chars().map(|c| find(face, c).map_or(0, |g| g.advance as i32)).sum())
}

/// Line height and ascent (distance from the top of a line to its baseline).
pub fn metrics(font: Font) -> (i32, i32) {
    with(font, |face, _| (face.line as i32, face.ascent as i32))
}

/// Draws `s` with its top-left corner at (`x`, `y`). Returns the width.
pub fn draw(c: &mut Canvas, font: Font, x: i32, y: i32, s: &str, color: u32) -> i32 {
    with(font, |face, data| {
        let base = y + face.ascent as i32;
        let mut pen = x;
        for ch in s.chars() {
            let Some(g) = find(face, ch) else { continue };
            if g.w > 0 {
                let start = data + g.data as usize;
                let len = g.w as usize * g.h as usize;
                let mask = &ATLAS[start..start + len];
                c.draw_mask(mask, g.w as i32, g.h as i32, pen + g.x as i32, base + g.y as i32, color);
            }
            pen += g.advance as i32;
        }
        pen - x
    })
}

/// Draws `s` centred horizontally on `cx`.
pub fn draw_centered(c: &mut Canvas, font: Font, cx: i32, y: i32, s: &str, color: u32) {
    let w = width(font, s);
    draw(c, font, cx - w / 2, y, s, color);
}

/// `s` shortened with "…" to fit `max` pixels.
pub fn fit(font: Font, s: &str, max: i32) -> alloc::string::String {
    if width(font, s) <= max {
        return s.into();
    }
    let mut out = alloc::string::String::new();
    for ch in s.chars() {
        out.push(ch);
        if width(font, &out) + width(font, "…") > max {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}

/// Draws one glyph of the terminal font in a cell; false if the font lacks it.
pub fn draw_cell_glyph(c: &mut Canvas, x: i32, y: i32, ch: char, color: u32) -> bool {
    with(Font::Mono, |face, data| {
        let Ok(i) = face.glyphs.binary_search_by_key(&(ch as u32), |g| g.code) else { return false };
        let g = face.glyphs[i];
        if g.w > 0 {
            let start = data + g.data as usize;
            let mask = &ATLAS[start..start + g.w as usize * g.h as usize];
            c.draw_mask(mask, g.w as i32, g.h as i32, x + g.x as i32, y + face.ascent as i32 + g.y as i32, color);
        }
        true
    })
}
