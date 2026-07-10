"""Title-screen splash: a 320x180 pixel-art dawn landscape.

One scene, drawn back-to-front: dawn sky and sun, far mountains, rolling
hills with fields and a farmstead, a factory with a smoking chimney, a
railway with a steam locomotive, and a sailing ship on the sea. The game
renders it full-screen with nearest-neighbor scaling; the title and menu
text are engine-side (pixel font), NOT baked into the art.

Deterministic like every other sprite — rerunning gen.py reproduces the
same image.
"""

import math
import random

import pixelkit
from pixelkit import Canvas

W, H = 320, 180

# Splash-only palette additions (registered as sp_*).
SPLASH_PAL = {
    "sky_hi":    "#2e3a52",  # night-blue upper sky
    "sky_mid":   "#4a4a63",
    "sky_low":   "#8a6a63",  # dawn haze
    "sky_glow":  "#c98d5e",  # horizon glow
    "sun":       "#f4d477",
    "sun_core":  "#f9e9b0",
    "mtn_far":   "#5d5a70",
    "mtn_near":  "#4d4a5d",
    "mtn_snow":  "#c9c4cc",
    "hill_far":  "#5f7a48",
    "hill_mid":  "#6d8a50",
    "hill_near": "#7ba652",
    "field_a":   "#a8b860",
    "field_b":   "#c9b36a",
    "field_c":   "#8fa055",
    "sea_deep":  "#33607f",
    "sea":       "#40729a",
    "sea_lt":    "#5c8cb0",
    "brick":     "#8a4a3a",
    "brick_dk":  "#6e392d",
    "roof":      "#4a4038",
    "smoke":     "#9a938c",
    "smoke_lt":  "#b5aea6",
    "rail_bed":  "#5b534c",
    "loco":      "#3a3530",
    "hull":      "#6b4320",
    "sail":      "#e8dcc0",
    "sail_sh":   "#c9bd9e",
}
for key, value in SPLASH_PAL.items():
    pixelkit.PAL[f"sp_{key}"] = value


def _c(name):
    return f"sp_{name}"


def _sky(cv, rng):
    # The glow band runs deep behind the hills so no seam of transparent
    # pixels can show between the terrain layers drawn on top.
    bands = [(0, 34, "sky_hi"), (34, 66, "sky_mid"), (66, 88, "sky_low"), (88, 150, "sky_glow")]
    for y0, y1, color in bands:
        cv.rect(0, y0, W - 1, y1 - 1, _c(color))
    # Dithered band seams soften the gradient without smoothness.
    for (_, y1, upper), (_, _, lower) in zip(bands, bands[1:]):
        for x in range(W):
            if (x * 7 + y1 * 13) % 3 == 0:
                cv.px(x, y1, _c(upper))
            if (x * 5 + y1 * 11) % 4 == 0:
                cv.px(x, y1 - 1, _c(lower))
    # Stars in the upper band.
    for _ in range(40):
        x, y = rng.randrange(W), rng.randrange(0, 40)
        cv.px(x, y, "snow" if rng.random() < 0.3 else _c("mtn_snow"))
    # Sun low over the sea, with a few ray dashes.
    sx, sy = 208, 82
    cv.disc(sx, sy, 11, _c("sun"))
    cv.disc(sx, sy, 7, _c("sun_core"))
    for dx in (-19, 15):
        cv.hline(sx + dx, sx + dx + 4, sy - 2, _c("sun"))
    cv.hline(sx - 24, sx - 16, sy + 4, _c("sun"))
    cv.hline(sx + 13, sx + 23, sy + 6, _c("sun"))


def _mountains(cv):
    # Far ridge: overlapping triangles across the left half.
    ridge = [(20, 66, 30), (58, 58, 34), (96, 70, 26), (128, 62, 30)]
    for apex_x, apex_y, half in ridge:
        cv.tri(apex_x, apex_y, 104, half, _c("mtn_far"))
    for apex_x, apex_y, half in ridge:
        cv.tri(apex_x, apex_y, apex_y + 6, max(3, half // 5), _c("mtn_snow"))
    # Taper the range off to the right instead of a hard cliff edge, and
    # drop one distant peak behind the factory corner for depth.
    for apex_x, apex_y, half in [(158, 78, 22), (184, 88, 14), (302, 84, 24)]:
        cv.tri(apex_x, apex_y, 104, half, _c("mtn_far"))
    # Near ridge, darker.
    for apex_x, apex_y, half in [(40, 76, 26), (86, 80, 22)]:
        cv.tri(apex_x, apex_y, 104, half, _c("mtn_near"))


def _hills_and_fields(cv, rng):
    # Rolling hill bands: each a cosine skyline filled all the way down —
    # the sea drawn later claims the bottom, so no gaps can open between
    # the bands.
    bands = [
        (108, 10, 0.020, "hill_far"),
        (120, 8, 0.026, "hill_mid"),
        (132, 7, 0.017, "hill_near"),
    ]
    for mid, amp, freq, color in bands:
        for x in range(W):
            top = int(mid - amp * math.cos(x * freq * math.tau + mid))
            cv.vline(x, top, H - 1, _c(color))
    # Field patchwork on the near hill, left side: strips that follow the
    # slope and run down into the hillside.
    for i, (x0, x1) in enumerate([(8, 40), (44, 78), (82, 112), (116, 142)]):
        color = ("field_a", "field_b", "field_c")[i % 3]
        for x in range(x0, x1):
            top = int(132 - 7 * math.cos(x * 0.017 * math.tau + 132)) + 3
            cv.vline(x, top, min(top + 20, 155), _c(color))
    # Furrow texture + scattered sheep.
    for _ in range(60):
        x, y = rng.randrange(4, 150), rng.randrange(138, 158)
        cv.px(x, y, _c("hill_mid"))
    for x, y in [(52, 141), (95, 144), (124, 140)]:
        cv.rect(x, y, x + 2, y + 1, "snow")
        cv.px(x - 1, y, "coal")


def _farmstead(cv):
    # Small house with a gabled roof on the near hill.
    x, y = 22, 128
    cv.rect(x, y + 4, x + 10, y + 10, "parch_sh")
    cv.tri(x + 5, y, y + 4, 7, _c("roof"))
    cv.rect(x + 4, y + 7, x + 6, y + 10, "wood_sh")
    cv.px(x + 2, y + 6, _c("sun_core"))
    cv.px(x + 8, y + 6, _c("sun_core"))


def _factory(cv, rng):
    # Brick mill with two chimneys, grounded on the near hill.
    x, y = 240, 116
    # Bluff under the mill so it roots into the hillside whatever the
    # cosine skyline does locally.
    cv.rect(x - 4, y + 20, x + 38, 152, _c("hill_near"))
    cv.rect(x, y, x + 34, y + 22, _c("brick"))
    for row in range(y + 2, y + 22, 4):
        for col in range(x + 1, x + 34, 6):
            cv.hline(col, col + 2, row, _c("brick_dk"))
    # Sawtooth roof.
    for i in range(4):
        cv.tri(x + 4 + i * 9, y - 5, y, 4, _c("roof"))
    # Lit windows.
    for wx in range(x + 3, x + 32, 7):
        cv.rect(wx, y + 6, wx + 2, y + 9, _c("sun"))
        cv.rect(wx, y + 14, wx + 2, y + 17, _c("sun_core"))
    # Chimneys + drifting smoke.
    for cx, height in [(x + 6, 16), (x + 24, 22)]:
        cv.rect(cx, y - height, cx + 3, y - 1, _c("brick_dk"))
        cv.hline(cx - 1, cx + 4, y - height, _c("roof"))
        px, py = cx + 1, y - height - 3
        for i in range(5):
            r = 1 + (i > 1) + (i > 3)
            cv.disc(px + rng.randrange(-1, 2) + i * 3, py - i * 4, r,
                    _c("smoke_lt") if i > 2 else _c("smoke"))


def _railway(cv):
    # Rail bed curving along the hill base, with a little locomotive.
    for x in range(0, W):
        y = 160 + (x // 52)
        cv.hline(x, x, y, _c("rail_bed"))
        cv.px(x, y + 1, _c("rail_bed"))
        if x % 4 == 0:
            cv.px(x, y + 2, "wood_sh")
    # Locomotive: boiler, cab, wheels, plume.
    x, y = 150, 152
    cv.rect(x, y + 3, x + 14, y + 8, _c("loco"))
    cv.rect(x + 14, y, x + 20, y + 8, _c("loco"))
    cv.rect(x + 15, y + 2, x + 18, y + 4, _c("sun"))
    cv.rect(x - 1, y + 1, x + 2, y + 3, "coal_lt")
    for wx in (x + 2, x + 7, x + 12, x + 17):
        cv.disc(wx, y + 9, 1, "coal")
    for i in range(4):
        cv.disc(x - 2 - i * 4, y - 2 - i * 2, 1 + (i > 1), _c("smoke_lt"))


def _sea_and_ship(cv, rng):
    # Sea fills the bottom; brighter toward the sun.
    cv.rect(0, 166, W - 1, H - 1, _c("sea"))
    cv.rect(0, 174, W - 1, H - 1, _c("sea_deep"))
    for _ in range(90):
        x, y = rng.randrange(W), rng.randrange(166, H)
        near_sun = 200 < x < 300 and y < 174
        cv.px(x, y, _c("sea_lt") if near_sun or rng.random() < 0.4 else _c("sea_deep"))
    # Sun glitter: sparse shimmer widening beneath the sun.
    for y in range(166, 177):
        half = 12 + (y - 166) * 3
        for _ in range(6):
            x = 208 + rng.randrange(-half, half + 1)
            cv.hline(x, x + rng.randrange(1, 3), y, _c("sun"))
    # Three-mast merchantman, silhouetted against the glitter.
    x, y = 190, 162
    cv.rect(x, y + 8, x + 26, y + 12, _c("hull"))
    cv.hline(x - 2, x, y + 8, _c("hull"))
    cv.hline(x + 26, x + 29, y + 8, _c("hull"))
    for mx, mh in [(x + 5, 14), (x + 13, 17), (x + 21, 13)]:
        cv.vline(mx, y + 8 - mh, y + 8, "wood_sh")
        cv.tri(mx, y + 8 - mh + 1, y + 5, mh // 3, _c("sail"))
        cv.vline(mx - 1, y + 8 - mh + 3, y + 4, _c("sail_sh"))
    cv.px(x + 13, y - 10, "red")


def title():
    cv = Canvas(W, H)
    rng = random.Random("Splash")
    _sky(cv, rng)
    _mountains(cv)
    _hills_and_fields(cv, rng)
    _farmstead(cv)
    _factory(cv, rng)
    _railway(cv)
    _sea_and_ship(cv, rng)
    return cv


SPLASH = [("splash/Title", title)]
