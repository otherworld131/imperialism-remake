"""Rail-link textures for the edge-based railway rendering (card #497).

`Track` is repeated along each hex-center-to-hex-center rail quad with
arc-length UVs (U tiles, V spans the quad width), so it must wrap seamlessly
in X: ties sit on an 8px grid (divides 32) and the ballast bed's ragged edge
uses a deterministic pattern that continues across the wrap. The band leaves
the top/bottom ~25% transparent so the drawn track reads narrower than the
quad. `Node` is a ballast disc drawn under every railhead hex center to hide
the butt joints where edge quads meet.
"""

import math
import random

from pixelkit import Canvas, SIZE

# Ballast bed vertical extent (inclusive). 14px core + outline rows keeps the
# painted track in the middle ~50% of the texture height.
BED_TOP = 9
BED_BOT = 22


def track():
    cv = Canvas()
    # Ballast bed with a gently ragged top/bottom edge. All edge detail is
    # periodic in x (period 8) so the texture wraps seamlessly.
    for y in range(BED_TOP, BED_BOT + 1):
        cv.hline(0, SIZE - 1, y, "grey")
    for x in range(SIZE):
        if x % 8 in (2, 5):
            cv.px(x, BED_TOP, "grey_dk")
        if x % 8 in (0, 3):
            cv.px(x, BED_BOT, "grey_dk")
        # Carve single-pixel notches off the edges on a sparse period.
        if x % 8 == 6:
            cv.g[BED_TOP][x] = None
        if x % 8 == 1:
            cv.g[BED_BOT][x] = None
    # Deterministic stone speckle inside the bed.
    rng = random.Random("RailTrack")
    for _ in range(46):
        x = rng.randrange(SIZE)
        y = rng.randrange(BED_TOP + 1, BED_BOT)
        cv.px(x, y, rng.choice(["grey_lt", "grey_dk"]))

    # Sleepers (ties): 3px wide wooden bars every 8px, proud of the rails.
    for tx in range(0, SIZE, 8):
        for x in range(tx, tx + 3):
            cv.vline(x % SIZE, BED_TOP + 1, BED_BOT - 1, "wood")
        cv.vline(tx % SIZE, BED_TOP + 1, BED_BOT - 1, "wood_lt")
        cv.vline((tx + 2) % SIZE, BED_TOP + 1, BED_BOT - 1, "wood_sh")

    # Two steel rails running full width: highlight row over shadowed row.
    for ry in (12, 18):
        cv.hline(0, SIZE - 1, ry, "steel_lt")
        cv.hline(0, SIZE - 1, ry + 1, "steel_dk")
        # Spike/fishplate glints where the rails cross the sleepers.
        for tx in range(0, SIZE, 8):
            cv.px((tx + 1) % SIZE, ry, "steel")

    cv.outline_silhouette()
    return cv


def node():
    cv = Canvas()
    # Ballast disc slightly wider than the track bed; hides quad butt joints.
    cv.disc(16, 16, 11, "grey")
    rng = random.Random("RailNode")
    for _ in range(34):
        a, r = rng.uniform(0, 2 * math.pi), rng.uniform(0, 10.0)
        cv.px(int(16 + r * math.cos(a)), int(16 + r * math.sin(a)),
              rng.choice(["grey_lt", "grey_dk"]))
    # Darker rim so the disc reads as a built-up bed, not a blob.
    for y in range(SIZE):
        for x in range(SIZE):
            if cv.g[y][x] == "grey":
                d2 = (x - 16) ** 2 + (y - 16) ** 2
                if 88 <= d2 <= 121:
                    cv.px(x, y, "grey_dk")
    cv.outline_silhouette()
    return cv


RAIL = [
    ("rail/Track", track),
    ("rail/Node", node),
]
