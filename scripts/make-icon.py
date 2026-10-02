#!/usr/bin/env python3
"""Generate assets/icon.png and assets/icon.ico from a source image.

The source is expected to be a rounded-square artwork on a checkerboard or
opaque background. The blue rounded square is detected automatically, cropped
and re-masked with clean transparent rounded corners so the icon looks correct
on any desktop background.

Usage:
    python3 scripts/make-icon.py [SOURCE]
"""

import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = ROOT / "assets" / "icon-source.webp"
OUT_PNG = ROOT / "assets" / "icon.png"
OUT_ICO = ROOT / "assets" / "icon.ico"

MASTER = 1024
MARGIN = 0.02
RADIUS_RATIO = 130 / 548
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def detect_bbox(img: Image.Image) -> tuple[int, int, int, int]:
    a = np.asarray(img.convert("RGB")).astype(int)
    r, g, b = a[..., 0], a[..., 1], a[..., 2]
    blue = (b > r + 25) & (b > 60) & (r < 150)
    ys, xs = np.where(blue)
    if len(xs) == 0:
        raise SystemExit("could not locate the blue icon in the source image")
    return int(xs.min()), int(ys.min()), int(xs.max()) + 1, int(ys.max()) + 1


def rounded_mask(size: int, radius: int, supersample: int = 4) -> Image.Image:
    big = size * supersample
    mask = Image.new("L", (big, big), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((0, 0, big - 1, big - 1), radius=radius * supersample, fill=255)
    return mask.resize((size, size), Image.LANCZOS)


def main() -> None:
    source = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SOURCE
    if not source.exists():
        raise SystemExit(f"missing source image: {source}")

    img = Image.open(source).convert("RGB")
    x0, y0, x1, y1 = detect_bbox(img)
    side = max(x1 - x0, y1 - y0)
    cx, cy = (x0 + x1) // 2, (y0 + y1) // 2
    crop = img.crop((cx - side // 2, cy - side // 2, cx + side // 2, cy + side // 2))

    inner = round(MASTER * (1 - 2 * MARGIN))
    art = crop.resize((inner, inner), Image.LANCZOS)
    mask = rounded_mask(inner, round(inner * RADIUS_RATIO))
    art.putalpha(mask)

    master = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    offset = (MASTER - inner) // 2
    master.alpha_composite(art, (offset, offset))

    master.save(OUT_PNG)
    master.save(OUT_ICO, sizes=ICO_SIZES)
    print(f"wrote {OUT_PNG} and {OUT_ICO} ({MASTER}x{MASTER}, source={source.name})")


if __name__ == "__main__":
    main()
