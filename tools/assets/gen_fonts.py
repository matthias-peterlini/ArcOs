#!/usr/bin/env python3
"""Renders the GUI fonts into an anti-aliased glyph atlas for the kernel.

The kernel has no font rasterizer: it draws text from alpha masks rendered
here, once, with Pillow. Output: system/kernel/assets/fonts.bin.

Format (little-endian):
    "ARCF" u16 version=1, u16 font count
    per font:  u16 size, u16 line height, i16 ascent, u16 glyph count,
               then per glyph: u32 code point, u8 width, u8 height,
               i8 x offset, i8 y offset (top of the mask relative to the
               baseline, negative is up), u8 advance, u8 pad,
               u32 offset of the mask in the data area
    data area: one byte of coverage (0-255) per mask pixel, row by row

Fonts: Plus Jakarta Sans (SIL Open Font License, in tools/assets/fonts, the
typeface of the ticly apps) and DejaVu Sans Mono (Bitstream Vera licence);
see system/kernel/assets/LICENSES.md.

Usage: tools/assets/gen_fonts.py [output]
"""
import struct
import sys

from PIL import Image, ImageDraw, ImageFont

import os

JAKARTA = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fonts", "PlusJakartaSans-")
DEJAVU = "/usr/share/fonts/truetype/dejavu/"

TEXT = [chr(c) for c in range(0x20, 0x7F)] + list("àèéìòùÀÈÉÌÒÙçÇ°·…–—€«»‘’“”×÷±²³•©−★→›")
DIGITS = list(" 0123456789:.,-")

# (file, pixel size, characters) — the order is the font id in the kernel.
FONTS = [
    (JAKARTA + "Medium.ttf", 14, TEXT),                 # 0 body
    (JAKARTA + "Bold.ttf", 14, TEXT),                   # 1 body, strong
    (JAKARTA + "Medium.ttf", 12, TEXT),                 # 2 small
    (JAKARTA + "Bold.ttf", 18, TEXT),                   # 3 heading
    (JAKARTA + "ExtraBold.ttf", 34, TEXT),              # 4 title
    (JAKARTA + "ExtraBold.ttf", 64, DIGITS),            # 5 big clock
    (DEJAVU + "DejaVuSansMono.ttf", 14, TEXT),          # 6 terminal
    (JAKARTA + "ExtraBold.ttf", 22, TEXT),              # 7 logo / buttons
    (JAKARTA + "Bold.ttf", 11, TEXT),                   # 8 spaced capitals ("OGGI")
]


def render(path, size, chars):
    font = ImageFont.truetype(path, size)
    ascent, descent = font.getmetrics()
    glyphs = []
    for ch in chars:
        left, top, right, bottom = font.getbbox(ch, anchor="ls")
        advance = round(font.getlength(ch))
        if ch == " ":
            # Jakarta's space is a hair narrow once advances are rounded to
            # whole pixels: words run together at the small sizes.
            advance = max(advance, round(size * 0.28))
        w, h = max(0, right - left), max(0, bottom - top)
        if w == 0 or h == 0:
            glyphs.append((ord(ch), 0, 0, 0, 0, advance, b""))
            continue
        img = Image.new("L", (w, h), 0)
        ImageDraw.Draw(img).text((-left, -top), ch, font=font, fill=255, anchor="ls")
        glyphs.append((ord(ch), w, h, left, top, advance, img.tobytes()))
    return ascent, ascent + descent, glyphs


# The faces of web pages (the browser), in sys:/system/webfonts.bin: every
# size a page is likely to ask for, regular and bold, and a monospaced face.
# The size field carries the style in its high bits (0x1000 bold, 0x2000
# monospaced); the file is gzipped.
WEB_TEXT = ([chr(c) for c in range(0x20, 0x7F)] + [chr(c) for c in range(0xA0, 0x180)]
            + list("ƒˆ˜‐‑‒–—―‘’‚‛“”„†‡•…‰′″‹›€™←↑→↓↔−∞≈≠≤≥✓✔✕✗★☆♥◆●○■□▲▶▼◀"))
WEB_SIZES = [10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 22, 24, 26, 28, 30, 32, 36, 40, 44, 48]
WEB_FONTS = ([(JAKARTA + "Medium.ttf", s, WEB_TEXT, s) for s in WEB_SIZES]
             + [(JAKARTA + "Bold.ttf", s, WEB_TEXT, 0x1000 | s) for s in WEB_SIZES]
             + [(DEJAVU + "DejaVuSansMono.ttf", s, WEB_TEXT, 0x2000 | s) for s in (11, 12, 13, 14, 16)])


def main():
    web = len(sys.argv) > 1 and sys.argv[1] == "--web"
    args = sys.argv[2:] if web else sys.argv[1:]
    out = args[0] if args else ("rootfs/system/webfonts.bin" if web else "system/kernel/assets/fonts.bin")
    fonts = WEB_FONTS if web else [(p, s, c, s) for p, s, c in FONTS]
    header = bytearray(b"ARCF" + struct.pack("<HH", 1, len(fonts)))
    data = bytearray()
    tables = bytearray()
    for path, size, chars, tag in fonts:
        ascent, line, glyphs = render(path, size, chars)
        tables += struct.pack("<HHhH", tag, line, ascent, len(glyphs))
        for code, w, h, x, y, adv, mask in glyphs:
            assert w < 256 and h < 256 and -128 <= x < 128 and -128 <= y < 128 and adv < 256, (path, size, chr(code))
            tables += struct.pack("<IBBbbBBI", code, w, h, x, y, adv, 0, len(data))
            data += mask
    blob = bytes(header + tables + data)
    if web:
        import gzip
        blob = gzip.compress(blob, 9, mtime=0)
    with open(out, "wb") as f:
        f.write(blob)
    print(f"{out}: {len(fonts)} fonts, {len(blob)} bytes")


if __name__ == "__main__":
    main()
