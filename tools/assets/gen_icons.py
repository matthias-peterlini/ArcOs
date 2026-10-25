#!/usr/bin/env python3
"""Draws the arcOS icon set into one atlas image.

Every icon is drawn from scratch here (shapes, gradients, symbols) at 512 px
and reduced to 128 px for smooth edges. The style: a rounded "squircle" with
a soft vertical gradient and a white or coloured symbol; documents are a page
with a folded corner. Small white glyphs (status bar, window buttons) go in
the same atlas.

Output: rootfs/system/gui/icons.qoi (a grid of 128 px cells, 8 per row) and
rootfs/system/gui/icons.idx (the icon names, one per line, in grid order).

Usage: tools/assets/gen_icons.py [output folder]
"""
import math
import os
import sys

import numpy as np
from PIL import Image, ImageChops, ImageDraw, ImageFilter, ImageFont

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qoi  # noqa: E402

S = 512          # drawing size
OUT = 128        # cell size in the atlas
COLS = 8
BOLD = "/usr/share/fonts/opentype/inter/Inter-Bold.otf"
INSET = 26       # squircle margin (room for the shadow)


def hexc(h, a=255):
    h = h.lstrip("#")
    return (int(h[0:2], 16), int(h[2:4], 16), int(h[4:6], 16), a)


def new():
    return Image.new("RGBA", (S, S), (0, 0, 0, 0))


def squircle_mask(inset=INSET, n=4.0):
    c = (S - 1) / 2
    r = S / 2 - inset
    y, x = np.mgrid[0:S, 0:S]
    v = (np.abs((x - c) / r) ** n + np.abs((y - c) / r) ** n)
    # A soft edge one pixel wide (at 512 px), sharpened by the reduction.
    a = np.clip((1.0 - v) * r / 2.0 + 0.5, 0, 1)
    return Image.fromarray((a * 255).astype(np.uint8), "L")


MASK = squircle_mask()


def gradient(top, bottom, size=S):
    t, b = np.array(hexc(top), float), np.array(hexc(bottom), float)
    k = np.linspace(0, 1, size)[:, None, None]
    arr = (t * (1 - k) + b * k).astype(np.uint8)
    arr = np.repeat(arr, size, axis=1)
    return Image.fromarray(arr, "RGBA")


def tile(top, bottom):
    """The squircle background with a gradient, a top sheen and a shadow."""
    img = new()
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    sh = Image.new("L", (S, S), 0)
    sh.paste(MASK, (0, 10))
    shadow.putalpha(sh.point(lambda v: v * 70 // 255).filter(ImageFilter.GaussianBlur(10)))
    img.alpha_composite(shadow)
    g = gradient(top, bottom)
    g.putalpha(MASK)
    img.alpha_composite(g)
    # Sheen: a faint lighter top half.
    sheen = Image.new("RGBA", (S, S), (255, 255, 255, 0))
    k = np.clip(1.0 - np.linspace(0, 1.6, S), 0, 1)[:, None] * 34
    sa = Image.fromarray(np.repeat(k, S, axis=1).astype(np.uint8), "L")
    sheen.putalpha(ImageChops.multiply(sa, MASK))
    img.alpha_composite(sheen)
    return img


def layer():
    im = new()
    return im, ImageDraw.Draw(im)


def over(base, top):
    base.alpha_composite(top)
    return base


def clip_to_tile(im):
    a = ImageChops.multiply(im.getchannel("A"), MASK)
    im.putalpha(a)
    return im


WHITE = (255, 255, 255, 255)

# ----------------------------------------------------------------------------
# Symbols


def folder(emblem=None):
    img = tile("#FFFFFF", "#E6EAF1")
    im, d = layer()
    d.rounded_rectangle((96, 140, 250, 220), 26, fill=hexc("#3D8BEF"))
    d.rounded_rectangle((96, 168, 416, 392), 30, fill=hexc("#3D8BEF"))
    front = gradient("#7CC0FF", "#3B8CF2").crop((0, 0, S, S))
    fm = Image.new("L", (S, S), 0)
    ImageDraw.Draw(fm).rounded_rectangle((96, 200, 416, 392), 30, fill=255)
    front.putalpha(fm)
    im.alpha_composite(front)
    over(img, im)
    if emblem:
        e, ed = layer()
        col = hexc("#1E5FC4", 235)
        emblem(ed, col, 256, 300)
        over(img, e)
    return img


def em_doc(d, c, x, y):
    d.rounded_rectangle((x - 34, y - 46, x + 34, y + 46), 8, fill=c)
    d.polygon([(x + 10, y - 46), (x + 34, y - 22), (x + 10, y - 22)], fill=hexc("#9CCBFF"))


def em_down(d, c, x, y):
    d.rectangle((x - 12, y - 50, x + 12, y + 8), fill=c)
    d.polygon([(x - 40, y + 2), (x + 40, y + 2), (x, y + 44)], fill=c)
    d.rounded_rectangle((x - 54, y + 50, x + 54, y + 62), 6, fill=c)


def em_pic(d, c, x, y):
    d.polygon([(x - 64, y + 40), (x - 18, y - 22), (x + 18, y + 24), (x + 34, y + 6), (x + 64, y + 40)], fill=c)
    d.ellipse((x + 20, y - 46, x + 46, y - 20), fill=c)


def em_music(d, c, x, y):
    d.rectangle((x - 22, y - 46, x - 10, y + 30), fill=c)
    d.rectangle((x + 30, y - 58, x + 42, y + 18), fill=c)
    d.polygon([(x - 22, y - 46), (x + 42, y - 58), (x + 42, y - 36), (x - 22, y - 24)], fill=c)
    d.ellipse((x - 54, y + 14, x - 6, y + 50), fill=c)
    d.ellipse((x - 2, y + 2, x + 46, y + 38), fill=c)


def em_video(d, c, x, y):
    d.polygon([(x - 26, y - 40), (x - 26, y + 40), (x + 40, y)], fill=c)


def em_home(d, c, x, y):
    d.polygon([(x, y - 54), (x - 60, y), (x + 60, y)], fill=c)
    d.rectangle((x - 42, y - 4, x + 42, y + 50), fill=c)
    d.rectangle((x - 12, y + 16, x + 12, y + 50), fill=hexc("#9CCBFF"))


def em_desk(d, c, x, y):
    d.rounded_rectangle((x - 58, y - 44, x + 58, y + 30), 8, fill=c)
    d.rectangle((x - 10, y + 30, x + 10, y + 44), fill=c)
    d.rounded_rectangle((x - 34, y + 42, x + 34, y + 52), 4, fill=c)


def trash():
    img = tile("#E2E5EA", "#A9AFB8")
    im, d = layer()
    w = (255, 255, 255, 240)
    d.rounded_rectangle((200, 112, 312, 140), 12, fill=w)
    d.rounded_rectangle((128, 132, 384, 172), 18, fill=w)
    d.polygon([(150, 186), (362, 186), (340, 404), (172, 404)], fill=w)
    for gx in (206, 256, 306):
        d.rounded_rectangle((gx - 9, 222, gx + 9, 370), 9, fill=hexc("#AEB4BD"))
    return over(img, im)


def lock():
    img = tile("#FFC83A", "#F5901C")
    im, d = layer()
    d.arc((168, 96, 344, 300), 180, 360, fill=WHITE, width=38)
    d.rectangle((168, 196, 206, 232), fill=WHITE)
    d.rectangle((306, 196, 344, 232), fill=WHITE)
    d.rounded_rectangle((132, 222, 380, 412), 40, fill=WHITE)
    d.ellipse((236, 280, 276, 320), fill=hexc("#F5A021"))
    d.rounded_rectangle((246, 300, 266, 356), 10, fill=hexc("#F5A021"))
    return over(img, im)


def wifi():
    img = tile("#4FD0FF", "#168BE8")
    im, d = layer()
    cx, cy = 256, 360
    for r in (210, 140, 72):
        d.arc((cx - r, cy - r, cx + r, cy + r), 225, 315, fill=WHITE, width=40)
    d.ellipse((cx - 30, cy - 30, cx + 30, cy + 30), fill=WHITE)
    return over(img, im)


def computer():
    img = tile("#FFFFFF", "#E8EBF1")
    im, d = layer()
    d.rounded_rectangle((104, 120, 408, 344), 26, fill=hexc("#1D1F24"))
    scr = gradient("#3E7BFF", "#2142D6")
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle((122, 138, 390, 326), 14, fill=255)
    scr.putalpha(m)
    im.alpha_composite(scr)
    d.polygon([(226, 344), (286, 344), (296, 392), (216, 392)], fill=hexc("#C8CDD6"))
    d.rounded_rectangle((180, 384, 332, 402), 8, fill=hexc("#B9BFC9"))
    return over(img, im)


def calendar():
    img = tile("#FFFFFF", "#EEF0F4")
    im, d = layer()
    d.rectangle((0, 0, S, 168), fill=hexc("#F0453A"))
    clip_to_tile(im)
    over(img, im)
    im, d = layer()
    for x in (184, 328):
        d.rounded_rectangle((x - 12, 92, x + 12, 158), 12, fill=hexc("#8E1E18"))
    for row in range(3):
        for col in range(5):
            x, y = 136 + col * 60, 236 + row * 56
            c = hexc("#F0453A") if (row, col) == (1, 2) else hexc("#C9CDD5")
            d.ellipse((x - 14, y - 14, x + 14, y + 14), fill=c)
    return over(img, im)


def activity():
    img = tile("#454852", "#1E2026")
    im, d = layer()
    pts = [(96, 270), (186, 270), (226, 170), (290, 350), (330, 250), (416, 250)]
    d.line(pts, fill=hexc("#38D66B"), width=38, joint="curve")
    for p in (pts[0], pts[-1]):
        d.ellipse((p[0] - 19, p[1] - 19, p[0] + 19, p[1] + 19), fill=hexc("#38D66B"))
    return over(img, im)


def screenshot():
    img = tile("#FFFFFF", "#F1E8F0")
    im, d = layer()
    c = hexc("#55595F")
    L, W = 76, 28
    for (x, y, sx, sy) in ((112, 112, 1, 1), (400, 112, -1, 1), (112, 400, 1, -1), (400, 400, -1, -1)):
        d.rounded_rectangle(sorted_box(x, y, x + sx * L, y + sy * W), 12, fill=c)
        d.rounded_rectangle(sorted_box(x, y, x + sx * W, y + sy * L), 12, fill=c)
    sq = gradient("#6DB6FF", "#2F7BEF")
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle((176, 176, 336, 336), 30, fill=255)
    sq.putalpha(m)
    im.alpha_composite(sq)
    return over(img, im)


def sorted_box(x0, y0, x1, y1):
    return (min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1))


def search():
    img = tile("#C77BFF", "#8338EC")
    im, d = layer()
    c = (255, 255, 255, 225)
    d.ellipse((128, 112, 324, 308), outline=c, width=40)
    d.line([(296, 284), (390, 378)], fill=c, width=48)
    d.ellipse((366, 354, 414, 402), fill=c)
    return over(img, im)


def mail():
    img = tile("#43A6FF", "#0B64DB")
    im, d = layer()
    d.rounded_rectangle((104, 150, 408, 362), 26, fill=hexc("#D9ECFF"))
    d.line([(118, 168), (256, 270), (394, 168)], fill=hexc("#8DBDF4"), width=18, joint="curve")
    return over(img, im)


def calculator():
    img = tile("#4A4D56", "#24262C")
    im, d = layer()
    keys = [((112, 112, 244, 244), "#6A6E78", "+"), ((268, 112, 400, 244), "#6A6E78", "x"),
            ((112, 268, 244, 400), "#6A6E78", "-"), ((268, 268, 400, 400), "#F7A21B", "=")]
    for box, col, sym in keys:
        d.rounded_rectangle(box, 32, fill=hexc(col))
        cx, cy = (box[0] + box[2]) // 2, (box[1] + box[3]) // 2
        if sym == "+":
            d.rounded_rectangle((cx - 40, cy - 11, cx + 40, cy + 11), 8, fill=WHITE)
            d.rounded_rectangle((cx - 11, cy - 40, cx + 11, cy + 40), 8, fill=WHITE)
        elif sym == "x":
            d.line([(cx - 30, cy - 30), (cx + 30, cy + 30)], fill=WHITE, width=22)
            d.line([(cx - 30, cy + 30), (cx + 30, cy - 30)], fill=WHITE, width=22)
        elif sym == "-":
            d.rounded_rectangle((cx - 40, cy - 11, cx + 40, cy + 11), 8, fill=WHITE)
        else:
            d.rounded_rectangle((cx - 38, cy - 28, cx + 38, cy - 8), 8, fill=WHITE)
            d.rounded_rectangle((cx - 38, cy + 8, cx + 38, cy + 28), 8, fill=WHITE)
    return over(img, im)


def music():
    img = tile("#FF7262", "#E52B3C")
    im, d = layer()
    em_music(d, WHITE, 262, 256)
    # A bolder note: draw it again scaled up.
    big, bd = layer()
    em_music(bd, WHITE, 256, 256)
    big = big.resize((S * 2, S * 2), Image.LANCZOS).crop((S // 2 - 8, S // 2 - 12, S // 2 * 3 - 8, S // 2 * 3 - 12))
    return over(img, big)


def photos():
    img = tile("#FFFFFF", "#ECEEF3")
    im = new()
    for (x, y, col) in ((216, 210, "#FFC23A"), (296, 210, "#FF4D5E"), (216, 290, "#2FD06A"), (296, 290, "#2F7BFF")):
        c, cd = layer()
        cd.ellipse((x - 82, y - 82, x + 82, y + 82), fill=hexc(col, 200))
        im.alpha_composite(c)
    return over(img, im)


def store():
    img = tile("#5A6DFF", "#2F2FCF")
    im, d = layer()
    d.arc((190, 92, 322, 224), 180, 360, fill=(235, 238, 255, 255), width=26)
    d.rounded_rectangle((122, 168, 390, 410), 40, fill=(235, 238, 255, 255))
    c = hexc("#3B3FE0")
    d.rectangle((244, 214, 268, 320), fill=c)
    d.polygon([(206, 302), (306, 302), (256, 362)], fill=c)
    return over(img, im)


def text_editor():
    img = tile("#FFFFFF", "#ECEEF3")
    im, d = layer()
    g = hexc("#3B3E45")
    for i, w in enumerate((290, 250, 270, 170)):
        y = 150 + i * 62
        d.rounded_rectangle((110, y, 110 + w, y + 30), 15, fill=g)
    d.rounded_rectangle((374, 300, 392, 404), 9, fill=hexc("#2F7BFF"))
    return over(img, im)


def browser():
    img = tile("#44C3FF", "#1576E0")
    im, d = layer()
    c = (235, 246, 255, 255)
    d.ellipse((108, 108, 404, 404), outline=c, width=22)
    d.ellipse((186, 108, 326, 404), outline=c, width=18)
    d.line([(256, 108), (256, 404)], fill=c, width=18)
    d.line([(108, 256), (404, 256)], fill=c, width=18)
    for y in (180, 332):
        half = int(math.sqrt(148 ** 2 - (y - 256) ** 2))
        d.line([(256 - half, y), (256 + half, y)], fill=c, width=16)
    return over(img, im)


def settings():
    img = tile("#E4E6EA", "#AEB2BA")
    im, d = layer()
    c = hexc("#8A8F99")
    cx = cy = 256
    pts = []
    for i in range(48):
        ang = i / 48 * 2 * math.pi
        r = 168 if (i // 3) % 2 == 0 else 136
        pts.append((cx + r * math.cos(ang), cy + r * math.sin(ang)))
    d.polygon(pts, fill=c)
    d.ellipse((cx - 104, cy - 104, cx + 104, cy + 104), fill=hexc("#E3E5E9"))
    d.ellipse((cx - 66, cy - 66, cx + 66, cy + 66), fill=c)
    d.ellipse((cx - 30, cy - 30, cx + 30, cy + 30), fill=hexc("#E3E5E9"))
    return over(img, im)


def terminal():
    img = tile("#4A4D55", "#1D1F24")
    im, d = layer()
    d.line([(136, 168), (232, 248), (136, 328)], fill=hexc("#C9CCD3"), width=40, joint="curve")
    d.rounded_rectangle((262, 316, 384, 352), 18, fill=WHITE)
    return over(img, im)


def files():
    img = tile("#4AA8FF", "#1867E0")
    im, d = layer()
    d.rounded_rectangle((100, 124, 412, 388), 30, fill=hexc("#E9F3FF"))
    d.rounded_rectangle((116, 140, 200, 372), 18, fill=hexc("#CFE2FA"))
    for i in range(4):
        d.rounded_rectangle((132, 166 + i * 34, 184, 180 + i * 34), 7, fill=hexc("#7FB0EE"))
    for r in range(2):
        for c in range(2):
            x, y = 222 + c * 92, 150 + r * 112
            d.rounded_rectangle((x, y, x + 78, y + 96), 18, fill=hexc("#2F7BEF"))
    return over(img, im)


def arc_logo():
    img = tile("#52C6FF", "#1F86EC")
    im, d = layer()
    d.arc((116, 6, 396, 330), 22, 158, fill=WHITE, width=70)
    return over(img, im)


def font_glyph(img, ch, size, color, dy=0):
    im, d = layer()
    f = ImageFont.truetype(BOLD, size)
    d.text((256, 256 + dy), ch, font=f, fill=color, anchor="mm")
    return over(img, im)


def info():
    img = tile("#4AA8FF", "#1462D8")
    im, d = layer()
    d.ellipse((222, 102, 290, 170), fill=WHITE)
    d.rounded_rectangle((224, 200, 288, 410), 32, fill=WHITE)
    return over(img, im)


def code():
    img = tile("#7C6CFF", "#3D2BE0")
    im, d = layer()
    w = (255, 255, 255, 240)
    d.line([(204, 160), (124, 256), (204, 352)], fill=w, width=36, joint="curve")
    d.line([(308, 160), (388, 256), (308, 352)], fill=w, width=36, joint="curve")
    d.line([(282, 136), (230, 376)], fill=w, width=32)
    return over(img, im)


def help_icon():
    return font_glyph(tile("#34E0B4", "#0FA88F"), "?", 300, WHITE, 10)


def power():
    img = tile("#FF6B5A", "#E3263A")
    im, d = layer()
    d.arc((120, 120, 392, 392), 300, 240, fill=WHITE, width=40)
    d.rounded_rectangle((236, 92, 276, 262), 20, fill=WHITE)
    return over(img, im)


def refresh():
    img = tile("#4BE07A", "#17A84A")
    im, d = layer()
    d.arc((124, 124, 388, 388), 300, 220, fill=WHITE, width=42)
    d.polygon([(318, 82), (402, 170), (296, 194)], fill=WHITE)
    return over(img, im)


def image_viewer():
    img = tile("#52C3FF", "#1E7EE6")
    im, d = layer()
    d.rounded_rectangle((100, 128, 412, 384), 26, fill=hexc("#EAF6FF"))
    sky = gradient("#BFE4FF", "#7CC2FF")
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle((118, 146, 394, 366), 16, fill=255)
    sky.putalpha(m)
    im.alpha_composite(sky)
    d.polygon([(118, 366), (210, 240), (280, 330), (320, 290), (394, 366)], fill=hexc("#1F6FD9"))
    d.ellipse((310, 168, 360, 218), fill=hexc("#FFFFFF"))
    return over(img, im)


def clock():
    img = tile("#FFFFFF", "#ECEEF3")
    im, d = layer()
    d.ellipse((112, 112, 400, 400), fill=hexc("#F7F8FA"), outline=hexc("#D7DBE2"), width=8)
    for i in range(12):
        a = i / 12 * 2 * math.pi
        r0, r1 = (112, 132) if i % 3 else (100, 132)
        d.line([(256 + r0 * math.sin(a), 256 - r0 * math.cos(a)), (256 + r1 * math.sin(a), 256 - r1 * math.cos(a))],
               fill=hexc("#3B3E45"), width=10)
    d.line([(256, 256), (256, 150)], fill=hexc("#2C2E33"), width=18)
    d.line([(256, 256), (330, 296)], fill=hexc("#2C2E33"), width=18)
    d.line([(256, 256), (196, 352)], fill=hexc("#F28A1B"), width=8)
    d.ellipse((240, 240, 272, 272), fill=hexc("#F28A1B"))
    return over(img, im)


def download():
    img = tile("#52E07E", "#15A548")
    im, d = layer()
    d.rounded_rectangle((232, 104, 280, 280), 24, fill=WHITE)
    d.polygon([(166, 236), (346, 236), (256, 330)], fill=WHITE)
    d.line([(124, 300), (124, 392), (388, 392), (388, 300)], fill=WHITE, width=40, joint="curve")
    return over(img, im)


def games():
    img = tile("#FF7FA2", "#E63E6D")
    im, d = layer()
    d.rounded_rectangle((96, 176, 416, 352), 88, fill=WHITE)
    c = hexc("#E64B76")
    d.rounded_rectangle((160, 248, 236, 276), 10, fill=c)
    d.rounded_rectangle((184, 224, 212, 300), 10, fill=c)
    d.ellipse((300, 222, 334, 256), fill=hexc("#F7B21E"))
    d.ellipse((338, 262, 372, 296), fill=hexc("#33C2F0"))
    return over(img, im)


def archive():
    img = tile("#FFC066", "#F2861E")
    im, d = layer()
    d.rounded_rectangle((104, 120, 408, 196), 24, fill=hexc("#FFF4E3"))
    d.rounded_rectangle((128, 186, 384, 398), 28, fill=hexc("#FFE6C2"))
    d.rounded_rectangle((204, 236, 308, 266), 15, fill=hexc("#E8892A"))
    return over(img, im)


def apps():
    img = tile("#4B4E57", "#24262C")
    im, d = layer()
    for r in range(2):
        for c in range(2):
            x, y = 144 + c * 120, 144 + r * 120
            d.rounded_rectangle((x, y, x + 104, y + 104), 26, fill=(236, 238, 243, 255))
    return over(img, im)


def usb():
    img = tile("#FFFFFF", "#ECEEF3")
    im, d = layer()
    body = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    bd = ImageDraw.Draw(body)
    bd.rounded_rectangle((206, 150, 306, 420), 36, fill=hexc("#2F7BEF"))
    bd.rounded_rectangle((222, 82, 290, 160), 10, fill=hexc("#C3C8D1"))
    bd.rectangle((236, 104, 250, 122), fill=hexc("#7E8590"))
    bd.rectangle((262, 104, 276, 122), fill=hexc("#7E8590"))
    body = body.rotate(-38, resample=Image.BICUBIC, center=(256, 256))
    return over(img, body)


def window_app():
    img = tile("#4AA8FF", "#1462D8")
    im, d = layer()
    d.rounded_rectangle((100, 128, 412, 384), 30, fill=hexc("#EAF4FF"))
    d.rounded_rectangle((100, 128, 412, 186), 30, fill=hexc("#BFDBFF"))
    d.rectangle((100, 160, 412, 186), fill=hexc("#BFDBFF"))
    for i, col in enumerate(("#FF5F57", "#FEBC2E", "#28C840")):
        d.ellipse((126 + i * 34, 146, 148 + i * 34, 168), fill=hexc(col))
    return over(img, im)


def user():
    img = tile("#7FB7FF", "#3567E8")
    im, d = layer()
    d.ellipse((186, 104, 326, 244), fill=WHITE)
    d.pieslice((116, 262, 396, 542), 180, 360, fill=WHITE)
    return clip_to_tile(over(img, im))


def keyboard():
    img = tile("#E4E6EA", "#AEB2BA")
    im, d = layer()
    d.rounded_rectangle((88, 158, 424, 354), 30, fill=hexc("#4A4E57"))
    for r in range(3):
        for c in range(7 if r < 2 else 5):
            x = 112 + c * 42 + (21 if r == 2 else 0) + (42 if r == 2 else 0)
            y = 180 + r * 42
            d.rounded_rectangle((x, y, x + 32, y + 32), 7, fill=hexc("#E9EBEF"))
    d.rounded_rectangle((174, 308, 338, 334), 8, fill=hexc("#E9EBEF"))
    return over(img, im)


# ----------------------------------------------------------------------------
# Documents


def page(content=None, band=None):
    img = new()
    shadow = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    sm = Image.new("L", (S, S), 0)
    ImageDraw.Draw(sm).rounded_rectangle((120, 70, 392, 452), 26, fill=90)
    shadow.putalpha(sm.filter(ImageFilter.GaussianBlur(12)))
    img.alpha_composite(shadow)
    im, d = layer()
    d.rounded_rectangle((112, 56, 400, 440), 26, fill=hexc("#FFFFFF"))
    d.polygon([(310, 56), (400, 146), (400, 56)], fill=(0, 0, 0, 0))
    im2, d2 = layer()
    d2.rounded_rectangle((112, 56, 400, 440), 26, fill=hexc("#FFFFFF"))
    m = Image.new("L", (S, S), 255)
    ImageDraw.Draw(m).polygon([(306, 50), (406, 150), (406, 50)], fill=0)
    im2.putalpha(ImageChops.multiply(im2.getchannel("A"), m))
    img.alpha_composite(im2)
    fold, fd = layer()
    fd.polygon([(306, 56), (306, 132), (382, 150), (400, 150)], fill=hexc("#DCE1E9"))
    fd.rounded_rectangle((306, 56, 400, 150), 18, fill=hexc("#E3E7EE"))
    fm = Image.new("L", (S, S), 0)
    ImageDraw.Draw(fm).polygon([(306, 56), (306, 150), (400, 150)], fill=255)
    fold.putalpha(ImageChops.multiply(fold.getchannel("A"), fm))
    img.alpha_composite(fold)
    if content:
        c, cd = layer()
        content(cd, c)
        img.alpha_composite(c)
    return img


def lines(color, top=196, n=5):
    def draw(d, _im):
        for i in range(n):
            w = 200 if i % 3 != 2 else 140
            d.rounded_rectangle((154, top + i * 42, 154 + w, top + i * 42 + 18), 9, fill=hexc(color))
    return draw


def pdf_content(d, im):
    lines("#D9DDE4", 170, 3)(d, im)
    d.rounded_rectangle((140, 310, 372, 380), 16, fill=hexc("#F0453A"))
    d.rounded_rectangle((170, 336, 260, 354), 8, fill=hexc("#FFD1CD"))


def image_content(d, im):
    d.polygon([(150, 360), (226, 240), (288, 320), (318, 288), (362, 360)], fill=hexc("#2F7BEF"))
    d.polygon([(150, 360), (226, 240), (250, 278), (190, 360)], fill=hexc("#5FA0FF"))
    d.ellipse((300, 180, 344, 224), fill=hexc("#FFC23A"))


def zip_content(d, im):
    for i in range(7):
        y = 70 + i * 34
        x = 248 if i % 2 == 0 else 264
        d.rectangle((x, y, x + 16, y + 18), fill=hexc("#B6BCC6"))
    d.rounded_rectangle((236, 318, 276, 386), 10, fill=hexc("#F2A93B"))


def code_content(d, im):
    c = hexc("#6B5BFF")
    d.line([(214, 208), (164, 262), (214, 316)], fill=c, width=24, joint="curve")
    d.line([(298, 208), (348, 262), (298, 316)], fill=c, width=24, joint="curve")
    d.line([(272, 196), (240, 328)], fill=c, width=20)


def audio_content(d, im):
    d.ellipse((176, 196, 336, 356), fill=hexc("#FF5C93"))
    em_music(d, WHITE, 258, 276)


def script_content(d, im):
    d.rounded_rectangle((150, 200, 362, 352), 22, fill=hexc("#E9F3FF"))
    d.arc((196, 196, 316, 316), 20, 160, fill=hexc("#1F86EC"), width=22)


def exe_content(d, im):
    d.rounded_rectangle((150, 200, 362, 352), 18, fill=hexc("#2F7BEF"))
    d.rectangle((150, 236, 362, 244), fill=hexc("#BFDBFF"))


# ----------------------------------------------------------------------------
# Small white glyphs (no tile)


def g_volume():
    im, d = layer()
    d.polygon([(110, 200), (180, 200), (270, 120), (270, 392), (180, 312), (110, 312)], fill=WHITE)
    d.arc((210, 160, 370, 352), 300, 60, fill=WHITE, width=30)
    d.arc((190, 100, 430, 412), 300, 60, fill=WHITE, width=30)
    return im


def g_wifi():
    im, d = layer()
    cx, cy = 256, 400
    for r in (280, 190, 100):
        d.arc((cx - r, cy - r, cx + r, cy + r), 225, 315, fill=WHITE, width=46)
    d.ellipse((cx - 36, cy - 36, cx + 36, cy + 36), fill=WHITE)
    return im


def g_battery():
    im, d = layer()
    d.rounded_rectangle((70, 170, 410, 342), 40, outline=WHITE, width=24)
    d.rounded_rectangle((420, 222, 452, 290), 12, fill=WHITE)
    d.rounded_rectangle((104, 204, 330, 308), 20, fill=WHITE)
    return im


def g_power():
    im, d = layer()
    d.arc((110, 110, 402, 402), 300, 240, fill=WHITE, width=44)
    d.rounded_rectangle((234, 64, 278, 250), 22, fill=WHITE)
    return im


def g_search():
    im, d = layer()
    d.ellipse((96, 96, 336, 336), outline=WHITE, width=44)
    d.line([(312, 312), (420, 420)], fill=WHITE, width=56)
    return im


def g_down():
    im, d = layer()
    d.line([(150, 210), (256, 316), (362, 210)], fill=WHITE, width=56, joint="curve")
    return im


def g_max():
    im, d = layer()
    d.line([(296, 140), (372, 140), (372, 216)], fill=WHITE, width=44, joint="curve")
    d.line([(216, 372), (140, 372), (140, 296)], fill=WHITE, width=44, joint="curve")
    d.line([(368, 144), (282, 230)], fill=WHITE, width=44)
    d.line([(144, 368), (230, 282)], fill=WHITE, width=44)
    return im


def g_close():
    im, d = layer()
    d.line([(160, 160), (352, 352)], fill=WHITE, width=56)
    d.line([(160, 352), (352, 160)], fill=WHITE, width=56)
    return im


def g_folder():
    im, d = layer()
    d.rounded_rectangle((60, 120, 240, 200), 30, fill=WHITE)
    d.rounded_rectangle((60, 160, 452, 400), 40, fill=WHITE)
    return im


def g_back():
    im, d = layer()
    d.line([(310, 130), (180, 256), (310, 382)], fill=WHITE, width=60, joint="curve")
    return im


def g_cloud():
    im, d = layer()
    d.ellipse((90, 230, 250, 390), fill=WHITE)
    d.ellipse((170, 140, 370, 340), fill=WHITE)
    d.ellipse((290, 230, 430, 370), fill=WHITE)
    d.rounded_rectangle((150, 290, 380, 390), 50, fill=WHITE)
    return im


ICONS = [
    ("folder", lambda: folder()),
    ("folder-documents", lambda: folder(em_doc)),
    ("folder-downloads", lambda: folder(em_down)),
    ("folder-pictures", lambda: folder(em_pic)),
    ("folder-music", lambda: folder(em_music)),
    ("folder-videos", lambda: folder(em_video)),
    ("folder-home", lambda: folder(em_home)),
    ("folder-desktop", lambda: folder(em_desk)),
    ("trash", trash),
    ("lock", lock),
    ("wifi", wifi),
    ("computer", computer),
    ("calendar", calendar),
    ("activity", activity),
    ("screenshot", screenshot),
    ("search", search),
    ("mail", mail),
    ("calculator", calculator),
    ("music", music),
    ("photos", photos),
    ("store", store),
    ("text-editor", text_editor),
    ("browser", browser),
    ("settings", settings),
    ("terminal", terminal),
    ("files", files),
    ("arc", arc_logo),
    ("info", info),
    ("code", code),
    ("help", help_icon),
    ("power", power),
    ("refresh", refresh),
    ("image-viewer", image_viewer),
    ("clock", clock),
    ("download", download),
    ("games", games),
    ("archive", archive),
    ("apps", apps),
    ("usb", usb),
    ("window", window_app),
    ("user", user),
    ("keyboard", keyboard),
    ("file", lambda: page(lines("#C9CED6"))),
    ("file-text", lambda: page(lines("#6FA3F2"))),
    ("file-pdf", lambda: page(pdf_content)),
    ("file-image", lambda: page(image_content)),
    ("file-archive", lambda: page(zip_content)),
    ("file-code", lambda: page(code_content)),
    ("file-audio", lambda: page(audio_content)),
    ("file-script", lambda: page(script_content)),
    ("file-exe", lambda: page(exe_content)),
    ("g-volume", g_volume),
    ("g-wifi", g_wifi),
    ("g-battery", g_battery),
    ("g-power", g_power),
    ("g-search", g_search),
    ("g-down", g_down),
    ("g-max", g_max),
    ("g-close", g_close),
    ("g-folder", g_folder),
    ("g-back", g_back),
    ("g-cloud", g_cloud),
]


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "rootfs/system/gui"
    os.makedirs(out, exist_ok=True)
    rows = (len(ICONS) + COLS - 1) // COLS
    atlas = Image.new("RGBA", (COLS * OUT, rows * OUT), (0, 0, 0, 0))
    for i, (name, draw) in enumerate(ICONS):
        icon = draw().resize((OUT, OUT), Image.LANCZOS)
        atlas.paste(icon, ((i % COLS) * OUT, (i // COLS) * OUT))
    data = qoi.encode(atlas.width, atlas.height, atlas.tobytes())
    with open(os.path.join(out, "icons.qoi"), "wb") as f:
        f.write(data)
    with open(os.path.join(out, "icons.idx"), "w") as f:
        f.write("\n".join(n for n, _ in ICONS) + "\n")
    if os.environ.get("PREVIEW"):
        atlas.save(os.environ["PREVIEW"])
    print(f"{out}/icons.qoi: {len(ICONS)} icons, {atlas.width}x{atlas.height}, {len(data)} bytes")


if __name__ == "__main__":
    main()
