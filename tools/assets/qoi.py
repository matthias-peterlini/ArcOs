"""A QOI ("Quite OK Image") encoder: https://qoiformat.org/qoi-specification.pdf

The kernel decodes QOI (gui/qoi.rs), a lossless format with a decoder of a
few dozen lines. Pixels are RGBA.
"""
import struct


def encode(width, height, rgba):
    """`rgba` is a bytes-like object of width*height*4 bytes."""
    out = bytearray(b"qoif" + struct.pack(">IIBB", width, height, 4, 0))
    index = [(0, 0, 0, 0)] * 64
    prev = (0, 0, 0, 255)
    run = 0
    n = width * height
    mv = memoryview(rgba)
    for i in range(n):
        px = tuple(mv[i * 4:i * 4 + 4])
        if px == prev:
            run += 1
            if run == 62 or i == n - 1:
                out.append(0xC0 | (run - 1))
                run = 0
            continue
        if run:
            out.append(0xC0 | (run - 1))
            run = 0
        r, g, b, a = px
        h = (r * 3 + g * 5 + b * 7 + a * 11) % 64
        if index[h] == px:
            out.append(h)
        else:
            index[h] = px
            if a == prev[3]:
                dr = (r - prev[0] + 128) % 256 - 128
                dg = (g - prev[1] + 128) % 256 - 128
                db = (b - prev[2] + 128) % 256 - 128
                dr_dg, db_dg = dr - dg, db - dg
                if -2 <= dr <= 1 and -2 <= dg <= 1 and -2 <= db <= 1:
                    out.append(0x40 | (dr + 2) << 4 | (dg + 2) << 2 | (db + 2))
                elif -32 <= dg <= 31 and -8 <= dr_dg <= 7 and -8 <= db_dg <= 7:
                    out += bytes([0x80 | (dg + 32), (dr_dg + 8) << 4 | (db_dg + 8)])
                else:
                    out += bytes([0xFE, r, g, b])
            else:
                out += bytes([0xFF, r, g, b, a])
        prev = px
    out += b"\x00" * 7 + b"\x01"
    return bytes(out)
