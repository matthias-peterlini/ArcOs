"""A QOI ("Quite OK Image") encoder and decoder:
https://qoiformat.org/qoi-specification.pdf

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


def decode(data):
    """Returns (width, height, rgba bytes)."""
    if data[:4] != b"qoif":
        raise ValueError("not a QOI image")
    width, height = struct.unpack(">II", data[4:12])
    out = bytearray(width * height * 4)
    index = [(0, 0, 0, 0)] * 64
    r, g, b, a = 0, 0, 0, 255
    p, run = 14, 0
    for i in range(width * height):
        if run:
            run -= 1
        else:
            op = data[p]
            p += 1
            if op == 0xFE:
                r, g, b = data[p:p + 3]
                p += 3
            elif op == 0xFF:
                r, g, b, a = data[p:p + 4]
                p += 4
            elif op >> 6 == 0:
                r, g, b, a = index[op]
            elif op >> 6 == 1:
                r = (r + (op >> 4 & 3) - 2) % 256
                g = (g + (op >> 2 & 3) - 2) % 256
                b = (b + (op & 3) - 2) % 256
            elif op >> 6 == 2:
                dg = (op & 0x3F) - 32
                nxt = data[p]
                p += 1
                r = (r + dg + (nxt >> 4) - 8) % 256
                g = (g + dg) % 256
                b = (b + dg + (nxt & 15) - 8) % 256
            else:
                run = op & 0x3F
            index[(r * 3 + g * 5 + b * 7 + a * 11) % 64] = (r, g, b, a)
        out[i * 4:i * 4 + 4] = bytes((r, g, b, a))
    return width, height, bytes(out)
