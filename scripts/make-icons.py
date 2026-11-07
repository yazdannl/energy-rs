#!/usr/bin/env python3
"""Generate the add-on icon (128x128) and logo (250x100) without external tooling.

Deterministic, dependency-free (zlib + struct only):

    python3 scripts/make-icons.py

Writes energy_rs/icon.png and energy_rs/logo.png: an emerald lightning bolt on a dark rounded
square, drawn with 4x4 supersampling for clean edges.
"""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SS = 4  # supersampling factor per axis

BG = (15, 23, 42)  # slate-900
FG = (52, 211, 153)  # emerald-400

# Lightning bolt, normalized 0..1 coordinates (y down), clockwise.
BOLT = [
    (0.600, 0.050),
    (0.245, 0.560),
    (0.480, 0.560),
    (0.400, 0.950),
    (0.775, 0.430),
    (0.525, 0.430),
]


def in_polygon(x: float, y: float, poly: list[tuple[float, float]]) -> bool:
    inside = False
    j = len(poly) - 1
    for i, (xi, yi) in enumerate(poly):
        xj, yj = poly[j]
        if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
            inside = not inside
        j = i
    return inside


def rounded_rect_sdf(px: float, py: float, hw: float, hh: float, r: float) -> float:
    """Signed distance to a rounded rectangle centred at the origin."""
    qx = abs(px) - hw + r
    qy = abs(py) - hh + r
    outside = (max(qx, 0.0) ** 2 + max(qy, 0.0) ** 2) ** 0.5
    inside = min(max(qx, qy), 0.0)
    return outside + inside - r


def render(width: int, height: int, mark_scale: float, background: bool) -> bytes:
    """Render RGBA rows (no filter bytes) for a w x h canvas."""
    rows = bytearray()
    cx, cy = width / 2.0, height / 2.0
    side = min(width, height) * mark_scale  # bolt box is side x side, centred
    hw, hh, radius = width / 2.0, height / 2.0, min(width, height) * 0.22

    for y in range(height):
        for x in range(width):
            bg_cov = 0
            fg_cov = 0
            for sy in range(SS):
                for sx in range(SS):
                    px = x + (sx + 0.5) / SS
                    py = y + (sy + 0.5) / SS
                    if background and rounded_rect_sdf(px - cx, py - cy, hw, hh, radius) <= 0:
                        bg_cov += 1
                    nx = (px - 0.5 * (width - side)) / side
                    ny = (py - 0.5 * (height - side)) / side
                    if 0.0 <= nx <= 1.0 and 0.0 <= ny <= 1.0 and in_polygon(nx, ny, BOLT):
                        fg_cov += 1
            samples = SS * SS
            fg_a = fg_cov / samples
            bg_a = bg_cov / samples if background else 0.0
            if fg_a >= bg_a:
                r, g, b = FG
                alpha = bg_a + (1.0 - bg_a) * fg_a
            else:
                r, g, b = BG
                alpha = bg_a
            rows += bytes((r, g, b, round(alpha * 255)))
    return bytes(rows)


def write_png(path: Path, width: int, height: int, pixels: bytes) -> None:
    stride = width * 4
    raw = bytearray()
    for y in range(height):
        raw.append(0)  # filter type 0 (None)
        raw += pixels[y * stride : (y + 1) * stride]

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)  # 8-bit RGBA
    path.write_bytes(
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def main() -> None:
    write_png(ROOT / "energy_rs" / "icon.png", 128, 128, render(128, 128, 0.62, True))
    # Logo: 250x100, transparent background so it sits on the add-on store's own background.
    write_png(ROOT / "energy_rs" / "logo.png", 250, 100, render(250, 100, 0.56, False))


if __name__ == "__main__":
    main()
