#!/usr/bin/env python3
"""Paints the arcOS wallpapers.

Every wallpaper is generated here from noise and gradients: a sky, a range of
rocky peaks lit by the low sun, a band of fog and a forest of firs in front.
Three moods share the same landscape: "dolomiti" (sunset), "alba" (dawn) and
"notte" (night).

Output: rootfs/system/gui/wallpapers/<name>.qoi (1280x800).

Usage: tools/assets/gen_wallpapers.py [output folder]
"""
import os
import sys

import numpy as np
from PIL import Image, ImageFilter

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qoi  # noqa: E402

W, H = 1280, 800


def noise1d(n, octaves, seed, base=8):
    rng = np.random.default_rng(seed)
    out = np.zeros(n)
    amp, freq, total = 1.0, base, 0.0
    for _ in range(octaves):
        pts = rng.random(freq + 1)
        xs = np.linspace(0, freq, n)
        i = np.floor(xs).astype(int).clip(0, freq - 1)
        t = xs - i
        t = t * t * (3 - 2 * t)
        out += amp * (pts[i] * (1 - t) + pts[i + 1] * t)
        total += amp
        amp *= 0.5
        freq *= 2
    return out / total


def noise2d(h, w, scale, seed, octaves=4):
    rng = np.random.default_rng(seed)
    out = np.zeros((h, w))
    amp, total = 1.0, 0.0
    for o in range(octaves):
        gh, gw = max(2, int(h / scale * 2 ** o)), max(2, int(w / scale * 2 ** o))
        g = rng.random((gh, gw)).astype(np.float32)
        img = Image.fromarray((g * 255).astype(np.uint8), "L").resize((w, h), Image.BICUBIC)
        out += amp * (np.asarray(img, float) / 255)
        total += amp
        amp *= 0.5
    return out / total


def lerp(a, b, t):
    return a + (b - a) * t


def vgrad(stops):
    """A vertical gradient through (position, rgb) stops, as an HxWx3 array."""
    ys = np.linspace(0, 1, H)
    col = np.zeros((H, 3))
    for c in range(3):
        col[:, c] = np.interp(ys, [p for p, _ in stops], [rgb[c] for _, rgb in stops])
    return np.repeat(col[:, None, :], W, axis=1)


def hexrgb(h):
    h = h.lstrip("#")
    return np.array([int(h[i:i + 2], 16) for i in (0, 2, 4)], float)


MOODS = {
    "dolomiti": dict(
        sky=[(0.0, "#4B3C6E"), (0.30, "#B97A9A"), (0.52, "#F1A98E"), (0.70, "#F6C7A2")],
        lit="#E8662F", lit_hi="#FFB067", shade="#5C2E35", fog="#F4D9D3", fog_a=0.85,
        trees="#14261D", trees_far="#4B5A55", clouds="#FFC7B8", stars=False),
    "alba": dict(
        sky=[(0.0, "#7FA4D6"), (0.35, "#C9C3E3"), (0.55, "#F7D6D2"), (0.72, "#FFE9D6")],
        lit="#E39A7C", lit_hi="#FFD2B0", shade="#6A5A73", fog="#F7EFF2", fog_a=0.9,
        trees="#1B2C2A", trees_far="#6F7F84", clouds="#FFFFFF", stars=False),
    "notte": dict(
        sky=[(0.0, "#070B1E"), (0.40, "#16224A"), (0.60, "#2D3A6B"), (0.72, "#3C4877")],
        lit="#53618F", lit_hi="#8C9BC9", shade="#1C2140", fog="#5A6796", fog_a=0.55,
        trees="#060A14", trees_far="#1E2645", clouds="#3A4677", stars=True),
}


def paint(mood):
    m = MOODS[mood]
    img = vgrad([(p, hexrgb(c)) for p, c in m["sky"]])

    # Clouds: soft streaks in the upper sky.
    cl = noise2d(H, W, 260, 7, 5)
    cl = np.clip((cl - 0.52) * 3.2, 0, 1)
    cl *= np.clip(1 - np.linspace(0, 1.7, H), 0, 1)[:, None]
    img = lerp(img, hexrgb(m["clouds"]), cl[..., None] * 0.55)

    if m["stars"]:
        rng = np.random.default_rng(3)
        n = 700
        ys, xs = rng.integers(0, int(H * 0.55), n), rng.integers(0, W, n)
        br = rng.random(n) ** 3
        for y, x, b in zip(ys, xs, br):
            img[y, x] = lerp(img[y, x], np.array([255, 255, 255.]), 0.4 + 0.6 * b)

    xs = np.arange(W)
    rows = np.arange(H)[:, None]

    # Far ridge, hazy.
    far = H * 0.48 + noise1d(W, 6, 11, 6) * H * 0.16
    far_mask = rows >= far[None, :]
    far_col = lerp(hexrgb(m["shade"]), hexrgb(m["sky"][-1][1]), 0.55)
    img = np.where(far_mask[..., None], far_col, img)

    # Main peaks: tall on the right, with vertical rock streaks.
    base = noise1d(W, 7, 21, 5)
    jag = noise1d(W, 8, 33, 40)
    spikes = np.abs(noise1d(W, 6, 44, 90) - 0.5) * 2
    # Dolomite towers: steep walls and broken tops.
    tower = np.clip(np.exp(-((xs - W * 0.74) / (W * 0.13)) ** 2) * 1.25, 0, 1) ** 1.6 * 0.52
    tower += np.clip(np.exp(-((xs - W * 0.20) / (W * 0.09)) ** 2) * 1.3, 0, 1) ** 1.6 * 0.30
    tower += np.clip(np.exp(-((xs - W * 0.86) / (W * 0.05)) ** 2) * 1.3, 0, 1) ** 1.4 * 0.10
    ridge = H * 0.72 - (base * 0.22 + tower + (jag - 0.5) * 0.09 + spikes * 0.05 * (tower > 0.05)) * H * 0.80
    ridge = np.minimum(ridge, H * 0.70)
    mask = rows >= ridge[None, :]
    streak = noise2d(H, W, 40, 5, 3)
    streak = np.asarray(Image.fromarray((streak * 255).astype(np.uint8)).resize((W // 6, H)).resize((W, H)), float) / 255
    # Light: the left faces of the peaks catch the sun.
    slope = np.gradient(ridge)
    light = np.clip(0.55 - slope[None, :] * 0.08, 0, 1) * np.clip(1 - (rows - ridge[None, :]) / (H * 0.32), 0, 1)
    light = np.clip(light * (0.6 + 0.8 * streak), 0, 1)
    rock = lerp(hexrgb(m["shade"]), hexrgb(m["lit"]), light[..., None])
    rock = lerp(rock, hexrgb(m["lit_hi"]), (np.clip(light - 0.75, 0, 1) * 3)[..., None])
    img = np.where(mask[..., None], rock, img)

    # Fog: a bright band in the valley, thinning upwards.
    fogn = noise2d(H, W, 120, 9, 4)
    band = np.exp(-((rows - H * 0.70) / (H * 0.09)) ** 2) * (0.6 + 0.6 * fogn)
    band += np.clip((rows - H * 0.66) / (H * 0.25), 0, 1) * 0.5
    img = lerp(img, hexrgb(m["fog"]), np.clip(band * m["fog_a"], 0, 1)[..., None])

    # Forest: rows of firs, from the hazy back to the dark front.
    rng = np.random.default_rng(17)
    forest = Image.fromarray(img.clip(0, 255).astype(np.uint8), "RGB").convert("RGBA")
    for row, (y0, scale, col, alpha) in enumerate((
        (0.80, 0.45, m["trees_far"], 170), (0.86, 0.65, m["trees_far"], 220),
        (0.92, 0.95, m["trees"], 255), (1.02, 1.35, m["trees"], 255),
    )):
        layer = Image.new("RGBA", (W, H), (0, 0, 0, 0))
        from PIL import ImageDraw
        d = ImageDraw.Draw(layer)
        c = tuple(int(v) for v in hexrgb(col)) + (alpha,)
        x = -20
        while x < W + 20:
            h = (70 + rng.random() * 90) * scale
            w = h * (0.28 + rng.random() * 0.08)
            base_y = H * y0 + rng.random() * 18 * scale
            tiers = 6
            for t in range(tiers):
                ty = base_y - h * t / tiers
                tw = w * (1 - t / tiers) + 2
                d.polygon([(x - tw, ty), (x + tw, ty), (x, ty - h / tiers * 1.9)], fill=c)
            d.rectangle((x - 2 * scale, min(base_y - 4, H - 1), x + 2 * scale, H), fill=c)
            x += w * (0.9 + rng.random() * 0.9)
        if row >= 2:
            # Solid ground under the near rows.
            d.rectangle((0, min(int(H * y0 + 10 * scale), H - 1), W, H), fill=c)
        if row < 2:
            layer = layer.filter(ImageFilter.GaussianBlur(1.2))
        forest.alpha_composite(layer)
        # A little mist between the rows.
        if row < 3:
            mist = Image.new("RGBA", (W, H), tuple(int(v) for v in hexrgb(m["fog"])) + (0,))
            a = np.clip((np.arange(H) - H * (y0 + 0.02)) / (H * 0.1), 0, 1) * (1 - np.clip((np.arange(H) - H * (y0 + 0.08)) / (H * 0.1), 0, 1))
            mist.putalpha(Image.fromarray(np.repeat((a * 60)[:, None], W, axis=1).astype(np.uint8), "L"))
            forest.alpha_composite(mist)
    out = forest.convert("RGB").filter(ImageFilter.GaussianBlur(0.6))
    return out


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "rootfs/system/gui/wallpapers"
    os.makedirs(out, exist_ok=True)
    for name in MOODS:
        img = paint(name).convert("RGBA")
        data = qoi.encode(W, H, img.tobytes())
        with open(os.path.join(out, name + ".qoi"), "wb") as f:
            f.write(data)
        if os.environ.get("PREVIEW"):
            img.convert("RGB").save(os.path.join(os.environ["PREVIEW"], name + ".png"))
        print(f"{out}/{name}.qoi: {len(data)} bytes")


if __name__ == "__main__":
    main()
