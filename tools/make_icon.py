#!/usr/bin/env python3
"""Draw SubText's application icon and turn it into a macOS .icns.

    tools/make_icon.py

Writes dist/icon/SubText.iconset/*.png and dist/icon/SubText.icns.

There is no image library here on purpose: the icon is a handful of rounded
rectangles over a vertical gradient, which is little enough geometry to render
from an analytic signed distance field with the standard library alone. That
also gets proper anti-aliasing for free, at any size, without supersampling
four million pixels in the interpreter.

The design is a document with five lines of text, one of them in the accent
colour, and a caret at the end of the last — the same idea as CotEditor's icon,
which is a sheet of paper with a pen.
"""

from __future__ import annotations

import math
import os
import struct
import subprocess
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "dist", "icon")
ICONSET = os.path.join(OUT, "SubText.iconset")

# The artwork is inset from the canvas the way macOS icons are: the rounded
# tile occupies about 83% of the square and the rest is transparent padding.
MARGIN = 0.085
CORNER = 0.225  # as a fraction of the tile's width

GRADIENT_TOP = (0x2B, 0x2F, 0x3A)
GRADIENT_BOTTOM = (0x14, 0x17, 0x1E)
BAR_IDLE = (0x7C, 0x87, 0x98)
BAR_ACCENT = (0x4C, 0x8D, 0xFF)
CARET = (0xF0, 0xF3, 0xF8)

# Lines, in tile-relative coordinates: (x, y, width, height, accent?).
LINES = [
    (0.170, 0.235, 0.500, 0.052, False),
    (0.170, 0.335, 0.660, 0.052, True),
    (0.170, 0.435, 0.590, 0.052, False),
    (0.170, 0.535, 0.680, 0.052, False),
    (0.170, 0.635, 0.330, 0.052, False),
]
# The caret sits just past the end of the last line.
CARET_RECT = (0.545, 0.618, 0.030, 0.088)

SIZES = [16, 32, 64, 128, 256, 512, 1024]


def round_rect_distance(px, py, cx, cy, half_w, half_h, radius):
    """Signed distance from (px, py) to a rounded rectangle; negative inside."""
    qx = abs(px - cx) - (half_w - radius)
    qy = abs(py - cy) - (half_h - radius)
    ax = qx if qx > 0.0 else 0.0
    ay = qy if qy > 0.0 else 0.0
    outer = math.hypot(ax, ay)
    inner = qx if qx > qy else qy
    if inner > 0.0:
        inner = 0.0
    return outer + inner - radius


def coverage(distance):
    """Turn a distance into 0..1 coverage with a one-pixel-wide ramp."""
    value = 0.5 - distance
    if value <= 0.0:
        return 0.0
    if value >= 1.0:
        return 1.0
    return value


def blend(base, over, alpha):
    return (
        base[0] + (over[0] - base[0]) * alpha,
        base[1] + (over[1] - base[1]) * alpha,
        base[2] + (over[2] - base[2]) * alpha,
    )


def render(size):
    """Render the icon at `size` and return a list of RGBA byte rows."""
    tile = size * (1.0 - 2.0 * MARGIN)
    origin = size * MARGIN
    center = size / 2.0
    radius = CORNER * tile
    half = tile / 2.0

    # Line rectangles, pre-converted to pixel space.
    lines = []
    for x, y, w, h, accent in LINES:
        lines.append(
            (
                origin + x * tile + (w * tile) / 2.0,
                origin + y * tile + (h * tile) / 2.0,
                (w * tile) / 2.0,
                (h * tile) / 2.0,
                min(h * tile, w * tile) / 2.0,
                BAR_ACCENT if accent else BAR_IDLE,
            )
        )
    caret = (
        origin + CARET_RECT[0] * tile + (CARET_RECT[2] * tile) / 2.0,
        origin + CARET_RECT[1] * tile + (CARET_RECT[3] * tile) / 2.0,
        (CARET_RECT[2] * tile) / 2.0,
        (CARET_RECT[3] * tile) / 2.0,
        (CARET_RECT[2] * tile) / 2.0,
        CARET,
    )

    span = (GRADIENT_TOP[0] - GRADIENT_BOTTOM[0], GRADIENT_TOP[1] - GRADIENT_BOTTOM[1],
            GRADIENT_TOP[2] - GRADIENT_BOTTOM[2])

    rows = []
    for row in range(size):
        py = row + 0.5
        # The gradient runs down the tile, not down the canvas.
        gradient_t = (py - origin) / tile
        gradient_t = 0.0 if gradient_t < 0.0 else (1.0 if gradient_t > 1.0 else gradient_t)
        gradient = (
            GRADIENT_TOP[0] - span[0] * gradient_t,
            GRADIENT_TOP[1] - span[1] * gradient_t,
            GRADIENT_TOP[2] - span[2] * gradient_t,
        )
        line = bytearray()
        for column in range(size):
            px = column + 0.5

            tile_alpha = coverage(round_rect_distance(px, py, center, center, half, half, radius))
            if tile_alpha <= 0.0:
                line += b"\x00\x00\x00\x00"
                continue

            colour = gradient
            for cx, cy, hw, hh, r, bar_colour in lines:
                alpha = coverage(round_rect_distance(px, py, cx, cy, hw, hh, r))
                if alpha > 0.0:
                    colour = blend(colour, bar_colour, alpha)

            alpha = coverage(round_rect_distance(px, py, *caret[:5]))
            if alpha > 0.0:
                colour = blend(colour, caret[5], alpha)

            line += bytes(
                (
                    int(colour[0] + 0.5),
                    int(colour[1] + 0.5),
                    int(colour[2] + 0.5),
                    int(tile_alpha * 255.0 + 0.5),
                )
            )
        rows.append(line)
    return rows


def write_png(path, size, rows):
    def chunk(tag, payload):
        body = tag + payload
        return (
            struct.pack(">I", len(payload))
            + body
            + struct.pack(">I", zlib.crc32(body) & 0xFFFFFFFF)
        )

    raw = b"".join(b"\x00" + bytes(row) for row in rows)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(raw, 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as handle:
        handle.write(png)


def downsample(rows, source, target):
    """Box-filter `rows` from `source` to `target` pixels square."""
    factor = source // target
    area = factor * factor
    out = []
    for row in range(target):
        line = bytearray()
        for column in range(target):
            totals = [0, 0, 0, 0]
            for dy in range(factor):
                source_row = rows[row * factor + dy]
                base = (column * factor) * 4
                for dx in range(factor):
                    offset = base + dx * 4
                    totals[0] += source_row[offset]
                    totals[1] += source_row[offset + 1]
                    totals[2] += source_row[offset + 2]
                    totals[3] += source_row[offset + 3]
            line += bytes(
                (
                    (totals[0] + area // 2) // area,
                    (totals[1] + area // 2) // area,
                    (totals[2] + area // 2) // area,
                    (totals[3] + area // 2) // area,
                )
            )
        out.append(line)
    return out


# macOS wants these exact names, and pairs of sizes share a file.
ICONSET_NAMES = {
    16: ["icon_16x16.png"],
    32: ["icon_16x16@2x.png", "icon_32x32.png"],
    64: ["icon_32x32@2x.png"],
    128: ["icon_128x128.png"],
    256: ["icon_128x128@2x.png", "icon_256x256.png"],
    512: ["icon_256x256@2x.png", "icon_512x512.png"],
    1024: ["icon_512x512@2x.png"],
}


def main():
    os.makedirs(ICONSET, exist_ok=True)

    print(f"==> rendering {len(SIZES)} sizes into {ICONSET}")
    master = None
    master_size = max(SIZES)
    for size in sorted(SIZES, reverse=True):
        if size == master_size:
            rows = render(size)
            master = (size, rows)
        else:
            source_size, source_rows = master
            rows = downsample(source_rows, source_size, size)
        for name in ICONSET_NAMES[size]:
            write_png(os.path.join(ICONSET, name), size, rows)
        print(f"    {size:>4}x{size:<4} -> {', '.join(ICONSET_NAMES[size])}")

    icns = os.path.join(OUT, "SubText.icns")
    if sys.platform == "darwin":
        print(f"==> iconutil -> {icns}")
        subprocess.run(
            ["iconutil", "-c", "icns", ICONSET, "-o", icns],
            check=True,
        )
        print(f"    {os.path.getsize(icns)} bytes")
    else:
        print("note: iconutil is macOS-only; the .iconset is ready to convert elsewhere")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
