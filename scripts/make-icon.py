"""Renders assets/open-preview.ico at every Windows icon size.

Same artwork as src/icon.rs (the runtime window icon), drawn on a 64-unit grid
and rasterized per size so small icons stay crisp. Requires Pillow.
"""

import math
import pathlib

from PIL import Image

# Neutral: graphite tile, lit from the top left, with a pale (#e6e6e6) glyph.
A = (58, 58, 58)
B = (20, 20, 20)
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def edge(px, py, ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    return -((px - ax) * dy - (py - ay) * dx) / math.hypot(dx, dy)


def render(n):
    img = Image.new("RGBA", (n, n))
    s = n / 64.0  # pixels per design unit
    px = img.load()
    for y in range(n):
        for x in range(n):
            fx, fy = (x + 0.5) / s, (y + 0.5) / s
            half, radius = 30.0, 15.0
            qx = abs(fx - 32) - (half - radius)
            qy = abs(fy - 32) - (half - radius)
            outside = math.hypot(max(qx, 0), max(qy, 0)) + min(max(qx, qy), 0) - radius
            cover = min(max(0.5 - outside * s, 0.0), 1.0)
            if cover <= 0:
                continue
            t = (fx + fy) / 128.0
            c = [a + (b - a) * t for a, b in zip(A, B)]
            tri = min(
                edge(fx, fy, 24, 18, 46, 32),
                edge(fx, fy, 46, 32, 24, 46),
                edge(fx, fy, 24, 46, 24, 18),
            )
            white = min(max(tri * s + 0.5, 0.0), 1.0)
            rgb = [int(ci + (230 - ci) * white) for ci in c]
            px[x, y] = (*rgb, int(cover * 255))
    return img


if __name__ == "__main__":
    out = pathlib.Path(__file__).resolve().parent.parent / "assets" / "open-preview.ico"
    out.parent.mkdir(exist_ok=True)
    images = [render(n) for n in SIZES]
    images[-1].save(out, sizes=[(n, n) for n in SIZES], append_images=images[:-1])
    images[-1].save(out.with_suffix(".png"))
    print(f"wrote {out}")
