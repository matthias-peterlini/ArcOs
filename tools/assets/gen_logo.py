#!/usr/bin/env python3
"""Cleans up the arc wordmark for the kernel.

The source is the "ARC Base" badge (tools/assets/src/arc-logo.png): dark
letters on a white pill. Only "ARC" is kept. The letters are lifted out of
the pill, enlarged with a smooth filter and sharpened again, which gives a
clean coverage mask at a size the interface scales down from.

Output: system/kernel/assets/logo.bin — u16 width, u16 height, then one byte
of coverage per pixel, row by row.

Usage: tools/assets/gen_logo.py [output]
"""
import os
import struct
import sys

import numpy as np
from PIL import Image, ImageFilter

SRC = os.path.join(os.path.dirname(os.path.abspath(__file__)), "src", "arc-logo.png")
SCALE = 6


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "system/kernel/assets/logo.bin"
    a = np.asarray(Image.open(SRC).convert("RGBA"), float)
    lum = a[..., :3].mean(-1)
    ink = np.zeros_like(lum)
    # Inside the pill (between the first and the last white pixel of a row),
    # dark is ink.
    for y in range(lum.shape[0]):
        white = np.where(lum[y] > 200)[0]
        if len(white) >= 2:
            x0, x1 = white.min() + 1, white.max() - 1
            ink[y, x0:x1] = (255 - lum[y, x0:x1]) / 255
    # "ARC" ends where "Base" begins.
    ink[:, :19] = 0
    ink[:, 75:] = 0
    ys, xs = np.where(ink > 0.15)
    m = ink[ys.min() - 1:ys.max() + 2, xs.min() - 1:xs.max() + 2]
    h, w = m.shape
    big = Image.fromarray((m * 255).astype(np.uint8)).resize((w * SCALE, h * SCALE), Image.BICUBIC)
    big = np.asarray(big.filter(ImageFilter.GaussianBlur(SCALE * 0.35)), float) / 255
    t = np.clip((big - 0.42) / 0.16, 0, 1)
    t = t * t * (3 - 2 * t)
    mask = (t * 255).astype(np.uint8)
    with open(out, "wb") as f:
        f.write(struct.pack("<HH", mask.shape[1], mask.shape[0]) + mask.tobytes())
    print(f"{out}: {mask.shape[1]}x{mask.shape[0]}")


if __name__ == "__main__":
    main()
