#!/usr/bin/env python3
"""Downloads the photo wallpapers of the Dolomites from Wikimedia Commons.

Each photo is a Featured or Quality picture under a free licence (CC BY-SA);
the script crops it to 16:10, scales it to 1920x1200 and writes it as
<name>.qoi, plus CREDITS.txt with author, licence and source of each one
(the licences require it).

Output: rootfs/system/gui/wallpapers/<name>.qoi and CREDITS.txt.

Usage: tools/assets/fetch_photos.py [output folder]
"""
import io
import json
import os
import re
import sys
import urllib.parse
import urllib.request

from PIL import Image

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import qoi  # noqa: E402

W, H = 1920, 1200
API = "https://commons.wikimedia.org/w/api.php"
UA = {"User-Agent": "arcOS-wallpapers/1.0 (https://github.com/matthias-peterlini/ARCos)"}

# name: (Commons file, label, where the 16:10 crop sits: 0 = top/left,
# 0.5 = centre, 1 = bottom/right)
PHOTOS = {
    "tre-cime": ("File:Drei Zinnen Tre Cime di Lavaredo Dolomites.jpg", "Tre Cime di Lavaredo", 0.4),
    "sassolungo": ("File:Langkofel group from the Sella pass 2016.jpg", "Sassolungo dal passo Sella", 0.0),
    "seceda": ("File:Odles da Secëda Gherdëina.jpg", "Odle dal Seceda", 0.55),
    "montanaia": ("File:Val Montanaia.jpg", "Campanile di Val Montanaia", 0.1),
}


def fetch(url):
    return urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=60).read()


def info(title):
    q = urllib.parse.urlencode({
        "action": "query", "format": "json", "titles": title, "prop": "imageinfo",
        "iiprop": "url|extmetadata", "iiurlwidth": "3840",
        "iiextmetadatafilter": "LicenseShortName|LicenseUrl|Artist",
    })
    page = next(iter(json.loads(fetch(f"{API}?{q}"))["query"]["pages"].values()))
    return page["imageinfo"][0]


def crop(img, at):
    w, h = img.size
    if w * H > h * W:  # wider than 16:10: cut the sides
        cw = h * W // H
        x = round((w - cw) * at)
        box = (x, 0, x + cw, h)
    else:  # taller: cut top and bottom
        ch = w * H // W
        y = round((h - ch) * at)
        box = (0, y, w, y + ch)
    return img.crop(box).resize((W, H), Image.LANCZOS)


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "rootfs/system/gui/wallpapers"
    os.makedirs(out, exist_ok=True)
    credits = ["Photo wallpapers (from Wikimedia Commons), cropped and scaled.", ""]
    for name, (title, label, at) in PHOTOS.items():
        ii = info(title)
        md = ii["extmetadata"]
        # The first sentence of the author field (some carry a long note).
        artist = re.sub(r"<[^>]+>", "", md["Artist"]["value"]).strip().split("\n")[0]
        artist = re.sub(r"^This Photo was taken by ", "", artist).strip().rstrip(".")
        img = crop(Image.open(io.BytesIO(fetch(ii["thumburl"]))).convert("RGBA"), at)
        with open(os.path.join(out, f"{name}.qoi"), "wb") as f:
            f.write(qoi.encode(W, H, img.tobytes()))
        credits += [f"{name}.qoi: {label}", f"  {artist}, {md['LicenseShortName']['value']}",
                    f"  {ii['descriptionurl']}", f"  {md.get('LicenseUrl', {}).get('value', '')}", ""]
        print(f"{name}: {artist}, {md['LicenseShortName']['value']}")
    with open(os.path.join(out, "CREDITS.txt"), "w", encoding="utf-8") as f:
        f.write("\n".join(credits))


if __name__ == "__main__":
    main()
