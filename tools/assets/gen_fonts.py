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

TEXT = [chr(c) for c in range(0x20, 0x7F)] + list("àèéìòùÀÈÉÌÒÙçÇ°·…–—€«»‘’“”×÷±²³")
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
        w, h = max(0, right - left), max(0, bottom - top)
        if w == 0 or h == 0:
            glyphs.append((ord(ch), 0, 0, 0, 0, advance, b""))
            continue
        img = Image.new("L", (w, h), 0)
        ImageDraw.Draw(img).text((-left, -top), ch, font=font, fill=255, anchor="ls")
        glyphs.append((ord(ch), w, h, left, top, advance, img.tobytes()))
    return ascent, ascent + descent, glyphs


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "system/kernel/assets/fonts.bin"
    header = bytearray(b"ARCF" + struct.pack("<HH", 1, len(FONTS)))
    data = bytearray()
    tables = bytearray()
    for path, size, chars in FONTS:
        ascent, line, glyphs = render(path, size, chars)
        tables += struct.pack("<HHhH", size, line, ascent, len(glyphs))
        for code, w, h, x, y, adv, mask in glyphs:
            assert w < 256 and h < 256 and -128 <= x < 128 and -128 <= y < 128 and adv < 256, (path, size, chr(code))
            tables += struct.pack("<IBBbbBBI", code, w, h, x, y, adv, 0, len(data))
            data += mask
    blob = header + tables + data
    with open(out, "wb") as f:
        f.write(blob)
    print(f"{out}: {len(FONTS)} fonts, {len(blob)} bytes")


if __name__ == "__main__":
    main()
