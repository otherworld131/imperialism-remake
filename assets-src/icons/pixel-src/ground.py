"""Tileable 32x32 ground textures, one per terrain type plus Sea.

Unlike the icon sprites these fill every pixel and wrap seamlessly: the map
renderer repeats them across merged hex meshes with world-aligned UVs, so
column 31 must sit next to column 0 without a visible seam. All placement
is seeded per texture — regeneration is deterministic.

Base colors track `theme::terrain_color` in the presentation crate so the
pixel map keeps the same overall reading as the old flat fills.
"""

import random

import pixelkit
from pixelkit import Canvas, SIZE


def _shade(hex_color, factor):
    """Lighten (>1) or darken (<1) a #rrggbb color."""
    h = hex_color.lstrip("#")
    channels = [int(h[i:i + 2], 16) for i in (0, 2, 4)]
    channels = [max(0, min(255, round(c * factor))) for c in channels]
    return "#" + "".join(f"{c:02x}" for c in channels)


# terrain → base color (mirrors theme::terrain_color).
# "ForestScrub" is the washed-out ground for timberless forest tiles
# (card #540): same family as Forest, but grey-green and less vivid.
BASES = {
    "Grassland": "#a8b860",
    "Hills": "#9a8a68",
    "Forest": "#3a7a3a",
    "ForestScrub": "#5c7a52",
    "Mountain": "#7a7068",
    "Desert": "#d8c888",
    "Swamp": "#5a7a5a",
    "Tundra": "#b8c8d0",
    "Sea": "#4a88b8",
}

# Register base + light/dark shades in the shared palette so to_svg works.
for terrain, base in BASES.items():
    key = terrain.lower()
    pixelkit.PAL[f"g_{key}"] = base
    pixelkit.PAL[f"g_{key}_lt"] = _shade(base, 1.12)
    pixelkit.PAL[f"g_{key}_dk"] = _shade(base, 0.88)
    pixelkit.PAL[f"g_{key}_dk2"] = _shade(base, 0.76)


def _base_canvas(terrain):
    cv = Canvas()
    key = f"g_{terrain.lower()}"
    cv.rect(0, 0, SIZE - 1, SIZE - 1, key)
    return cv, key


def _wrap_px(cv, x, y, c):
    cv.px(x % SIZE, y % SIZE, c)


def _wrap_hdash(cv, x, y, length, c):
    for i in range(length):
        _wrap_px(cv, x + i, y, c)


def _speckle(cv, rng, count, colors, clump=1):
    """Scatter 1px (or clump×clump) dots, wrapping at the edges."""
    for _ in range(count):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        c = rng.choice(colors)
        for dy in range(clump):
            for dx in range(clump):
                _wrap_px(cv, x + dx, y + dy, c)


def grassland():
    cv, k = _base_canvas("Grassland")
    rng = random.Random("Grassland")
    _speckle(cv, rng, 46, [f"{k}_lt", f"{k}_dk"])
    # Grass tufts: short vertical darker blades.
    for _ in range(14):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_px(cv, x, y, f"{k}_dk")
        _wrap_px(cv, x, y + 1, f"{k}_dk2")
    # A few tiny straw-colored flowers keep it storybook-cute, not noisy.
    for _ in range(5):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_px(cv, x, y, "straw_lt")
    return cv


def hills():
    cv, k = _base_canvas("Hills")
    rng = random.Random("Hills")
    _speckle(cv, rng, 40, [f"{k}_lt", f"{k}_dk"])
    # Short diagonal contour dashes suggest folded ground.
    for _ in range(11):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        for i in range(3):
            _wrap_px(cv, x + i, y - (i // 2), f"{k}_dk2")
    return cv


def forest():
    cv, k = _base_canvas("Forest")
    rng = random.Random("Forest")
    _speckle(cv, rng, 52, [f"{k}_lt", f"{k}_dk"])
    # Dark underbrush clumps beneath the tree motifs layered on top.
    for _ in range(9):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_px(cv, x, y, f"{k}_dk2")
        _wrap_px(cv, x + 1, y, f"{k}_dk2")
        _wrap_px(cv, x, y + 1, f"{k}_dk")
    return cv


def forest_scrub():
    cv, k = _base_canvas("ForestScrub")
    rng = random.Random("ForestScrub")
    # Sparser + paler than Forest: fewer speckles, thinner underbrush.
    _speckle(cv, rng, 34, [f"{k}_lt", f"{k}_dk"])
    for _ in range(5):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_px(cv, x, y, f"{k}_dk2")
        _wrap_px(cv, x + 1, y, f"{k}_dk")
    return cv


def mountain():
    cv, k = _base_canvas("Mountain")
    rng = random.Random("Mountain")
    _speckle(cv, rng, 42, [f"{k}_lt", f"{k}_dk"])
    # Crag dashes plus the odd snow fleck.
    for _ in range(10):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_hdash(cv, x, y, 2, f"{k}_dk2")
        _wrap_px(cv, x + 1, y + 1, f"{k}_dk")
    for _ in range(6):
        _wrap_px(cv, rng.randrange(SIZE), rng.randrange(SIZE), "snow_sh")
    return cv


def desert():
    cv, k = _base_canvas("Desert")
    rng = random.Random("Desert")
    _speckle(cv, rng, 26, [f"{k}_lt", f"{k}_dk"])
    # Staggered ripple dashes: wind-combed sand.
    for row in range(4):
        y = row * 8 + 3
        for col in range(3):
            x = col * 11 + (row * 5) % 11 + rng.randrange(3)
            _wrap_hdash(cv, x, y + rng.randrange(2), 4, f"{k}_dk")
    return cv


def swamp():
    cv, k = _base_canvas("Swamp")
    rng = random.Random("Swamp")
    _speckle(cv, rng, 38, [f"{k}_lt", f"{k}_dk"])
    # Murky pools with a pale glint on their surface.
    for _ in range(7):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_hdash(cv, x, y, 3, f"{k}_dk2")
        _wrap_hdash(cv, x, y + 1, 2, f"{k}_dk2")
        _wrap_px(cv, x + 1, y, f"{k}_lt")
    return cv


def tundra():
    cv, k = _base_canvas("Tundra")
    rng = random.Random("Tundra")
    _speckle(cv, rng, 34, [f"{k}_lt", f"{k}_dk"])
    # Snow drifts and sparse frozen scrub.
    for _ in range(9):
        x, y = rng.randrange(SIZE), rng.randrange(SIZE)
        _wrap_hdash(cv, x, y, 3, "snow")
    for _ in range(6):
        _wrap_px(cv, rng.randrange(SIZE), rng.randrange(SIZE), "grey_lt")
    return cv


def sea():
    cv, k = _base_canvas("Sea")
    rng = random.Random("Sea")
    _speckle(cv, rng, 22, [f"{k}_dk"])
    # Classic staggered wave crests: light dash with a dark shadow under
    # the trailing edge, laid out on a loose grid so the swell reads calm.
    for row in range(4):
        y = row * 8 + 2
        for col in range(3):
            x = col * 11 + (row * 6) % 11 + rng.randrange(2)
            _wrap_hdash(cv, x, y, 3, f"{k}_lt")
            _wrap_hdash(cv, x + 1, y + 1, 2, f"{k}_dk")
    return cv


GROUND = [
    ("ground/Grassland", grassland),
    ("ground/Hills", hills),
    ("ground/Forest", forest),
    ("ground/ForestScrub", forest_scrub),
    ("ground/Mountain", mountain),
    ("ground/Desert", desert),
    ("ground/Swamp", swamp),
    ("ground/Tundra", tundra),
    ("ground/Sea", sea),
]
