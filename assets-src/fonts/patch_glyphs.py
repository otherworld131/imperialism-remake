"""Patch pixel-style UI glyphs into the bundled Jersey 15 font.

Jersey 15 (OFL) covers Latin but not the geometric UI glyphs the widgets
use (table sort arrows, dropdown chevron, modal close, checkmarks). This
script draws them as pixel-block outlines matched to the font's grid and
writes the patched TTF the game actually bundles.

Usage:
    python3 patch_glyphs.py <upstream-Jersey15-Regular.ttf> \
        ../../crates/presentation/assets/fonts/Jersey15-Regular.ttf

Deterministic: same input font → same output bytes (modulo fontTools
version). Rerun after upgrading the upstream font.
"""

import sys

from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

# Each glyph is a set of filled cells on a small grid, y-up, drawn in
# font units as (cell size = one "font pixel" of Jersey 15's design grid).
GLYPHS = {
    0x25B2: ("uni25B2", [  # ▲ 4-row staircase pyramid, 7 cells wide
        (3, 3, 1), (2, 2, 3), (1, 1, 5), (0, 0, 7),
    ]),
    0x25BC: ("uni25BC", [  # ▼
        (3, 0, 7), (2, 1, 5), (1, 2, 3), (0, 3, 1),
    ]),
}
# Diagonal glyphs as explicit cell lists: (col, row) with y-up rows.
CELLS = {
    0x2715: ("uni2715", [  # ✕ 5×5 pixel cross
        (0, 0), (1, 1), (2, 2), (3, 3), (4, 4),
        (4, 0), (3, 1), (1, 3), (0, 4),
    ]),
    0x2713: ("uni2713", [  # ✓ short left arm, long rising arm
        (0, 2), (1, 1), (2, 0), (3, 1), (4, 2), (5, 3),
    ]),
    0x21BB: ("uni21BB", [  # ↻ ring open at the top-right, arrow tip there
        (1, 0), (2, 0), (3, 0), (4, 0),
        (0, 1), (0, 2), (0, 3),
        (5, 1), (5, 2), (5, 3),
        (1, 4), (2, 4),
        (4, 4), (5, 4), (4, 5),
    ]),
    0x2192: ("uni2192", [  # → shaft + diagonal wedge head
        (0, 2), (1, 2), (2, 2), (3, 2), (4, 2), (5, 2), (6, 2),
        (4, 0), (5, 1), (5, 3), (4, 4),
    ]),
    0x21D2: ("uni21D2", [  # ⇒ double shaft, filled head
        (0, 1), (1, 1), (2, 1), (3, 1),
        (0, 3), (1, 3), (2, 3), (3, 3),
        (4, 0), (4, 1), (4, 2), (4, 3), (4, 4),
        (5, 1), (5, 2), (5, 3), (6, 2),
    ]),
    0x2191: ("uni2191", [  # ↑ vertical shaft, wedge head
        (2, 0), (2, 1), (2, 2), (2, 3), (2, 4), (2, 5),
        (1, 4), (3, 4), (0, 3), (4, 3),
    ]),
    0x25B6: ("uni25B6", [  # ▶ right-pointing staircase triangle
        (0, 0), (0, 1), (0, 2), (0, 3), (0, 4),
        (1, 1), (1, 2), (1, 3), (2, 2),
    ]),
    0x221E: ("uni221E", [  # ∞ two linked rings
        (1, 3), (2, 3), (4, 3), (5, 3),
        (0, 2), (3, 2), (6, 2),
        (0, 1), (3, 1), (6, 1),
        (1, 0), (2, 0), (4, 0), (5, 0),
    ]),
    0x2605: ("uni2605", [  # ★ chunky five-point star
        (2, 4),
        (1, 3), (2, 3), (3, 3),
        (0, 2), (1, 2), (2, 2), (3, 2), (4, 2),
        (1, 1), (2, 1), (3, 1),
        (0, 0), (4, 0),
    ]),
    0x2630: ("uni2630", [  # ☰ burger-menu trigram, three 7-wide bars
        (0, 4), (1, 4), (2, 4), (3, 4), (4, 4), (5, 4), (6, 4),
        (0, 2), (1, 2), (2, 2), (3, 2), (4, 2), (5, 2), (6, 2),
        (0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0),
    ]),
    0x0394: ("uni0394", [  # Δ outline triangle (distinct from filled ▲)
        (0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0),
        (1, 1), (5, 1), (2, 2), (4, 2), (3, 3), (3, 4),
    ]),
    0x26A0: ("uni26A0", [  # ⚠ outline triangle with a bang
        (0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0),
        (1, 1), (5, 1), (2, 2), (4, 2), (3, 3), (3, 4),
        (3, 1),
    ]),
    0x23F3: ("uni23F3", [  # ⏳ hourglass with sand in the lower bulb
        (0, 5), (1, 5), (2, 5), (3, 5), (4, 5),
        (1, 4), (2, 4), (3, 4),
        (2, 3), (2, 2),
        (1, 1), (2, 1), (3, 1),
        (0, 0), (1, 0), (2, 0), (3, 0), (4, 0),
    ]),
}


def rect(pen, x0, y0, x1, y1):
    pen.moveTo((x0, y0))
    pen.lineTo((x1, y0))
    pen.lineTo((x1, y1))
    pen.lineTo((x0, y1))
    pen.closePath()


def main(src, dst):
    font = TTFont(src)
    upem = font["head"].unitsPerEm
    # Jersey 15 is drawn on a coarse grid; one "pixel" ≈ upem / 8 reads at
    # the same weight as its letter strokes.
    p = upem // 8
    baseline_lift = p  # sit glyphs slightly above the baseline
    new_names = []

    def add_glyph(name, pen, width_cells):
        font["glyf"][name] = pen.glyph()
        advance = width_cells * p + p  # one pixel of right bearing
        font["hmtx"][name] = (advance, p // 2)
        new_names.append(name)

    # Triangles: rows of (row, inset, width_cells) on a 7-cell grid.
    for code, (name, rows) in GLYPHS.items():
        pen = TTGlyphPen(None)
        for row, inset, width in rows:
            x0 = p // 2 + inset * p
            y0 = baseline_lift + row * p
            rect(pen, x0, y0, x0 + width * p, y0 + p)
        add_glyph(name, pen, 7)
        for table in font["cmap"].tables:
            if table.isUnicode():
                table.cmap[code] = name

    # Cell-list glyphs.
    for code, (name, cells) in CELLS.items():
        pen = TTGlyphPen(None)
        width = max(c for c, _ in cells) + 1
        for col, row in cells:
            x0 = p // 2 + col * p
            y0 = baseline_lift + row * p
            rect(pen, x0, y0, x0 + p, y0 + p)
        add_glyph(name, pen, width)
        for table in font["cmap"].tables:
            if table.isUnicode():
                table.cmap[code] = name

    # glyf.__setitem__ already appended the new names to the font's live
    # glyph order — only the post name table needs a rebuild.
    if "post" in font:
        font["post"].extraNames = []
        font["post"].mapping = {}
        font["post"].glyphOrder = None
    font.save(dst)
    print(f"patched {len(new_names)} glyphs -> {dst}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
