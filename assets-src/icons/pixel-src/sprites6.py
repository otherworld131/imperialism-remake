"""Batch 6 — city tile art (cards #541): the map representation of
capitals. The country capital is a palace flanked by a couple of smaller
houses; a province capital is a cluster of small houses. These replace the
old gold-star / dot markers as the capital hexes' tile identity.
"""

from pixelkit import Canvas


def _house(c, x0, x1, roof_y, wall_y1, wall, wall_lt, roof, roof_sh,
           door=None, window=None):
    """Small pitched-roof house. `roof_y` is the eaves row (top of walls
    minus one); the roof rises from there to a centered ridge."""
    wall_y0 = roof_y + 1
    c.rect(x0, wall_y0, x1, wall_y1, wall)
    c.vline(x0, wall_y0, wall_y1, wall_lt)
    w = x1 - x0
    rh = max(2, (w + 1) // 3)
    for i in range(rh + 1):
        y = roof_y - i
        c.hline(x0 - 1 + i * (w // 2 + 1) // rh, x1 + 1 - i * (w // 2 + 1) // rh, y, roof)
    c.hline(x0 - 1, x1 + 1, roof_y, roof_sh)
    if door is not None:
        dx = door
        c.rect(dx, wall_y1 - 2, dx + 1, wall_y1, "wood_sh")
    if window is not None:
        c.px(window, wall_y0 + 1, "straw_lt")


def capital_city():
    """Country capital: columned palace with a gold dome and standard,
    fronted by a couple of smaller houses."""
    c = Canvas()
    # ── palace body ──
    c.rect(5, 10, 26, 17, "parch")
    c.rect(22, 11, 26, 17, "parch_sh")          # shaded right end
    c.hline(4, 27, 9, "parch")                   # cornice
    c.hline(4, 27, 10, "parch_dk")               # cornice shadow line
    # colonnade
    for x in (7, 10, 13, 18, 21, 24):
        c.vline(x, 11, 16, "parch_dk")
        c.vline(x + 1, 11, 16, "parch")
    # windows between columns
    for x in (9, 12, 20, 23):
        c.px(x, 12, "navy_dk")
    # portal
    c.rect(15, 13, 16, 17, "wood_sh")
    c.hline(15, 16, 13, "wood")
    # ── dome on a drum + standard ──
    c.rect(12, 7, 19, 8, "parch_sh")
    c.hline(12, 19, 7, "parch_dk")
    c.disc(15, 5, 3, "gold")
    c.px(14, 4, "gold_lt")
    c.px(13, 5, "gold_lt")
    c.px(17, 6, "gold_sh")
    c.vline(15, 1, 2, "wood_sh")                 # flag pole
    c.px(16, 1, "red")
    c.px(17, 1, "red")
    c.px(16, 2, "red_dk")
    # gold roofline trim over the wings
    c.hline(5, 11, 8, "gold_sh")
    c.hline(20, 26, 8, "gold_sh")
    # ── flanking houses (in front of the palace) ──
    _house(c, 2, 9, 20, 27, "wood", "wood_lt", "red", "red_dk",
           door=5, window=3)
    _house(c, 22, 29, 21, 28, "parch_sh", "parch", "wood", "wood_sh",
           door=25, window=27)
    c.outline_silhouette()
    return c


def province_town():
    """Province capital: a modest cluster of small houses."""
    c = Canvas()
    # back house (center, red roof + chimney)
    _house(c, 10, 20, 11, 18, "parch_sh", "parch", "red", "red_dk",
           window=15)
    c.vline(18, 7, 8, "coal")                    # chimney
    c.px(19, 6, "grey_lt")                       # smoke wisp
    # front-left house (timber walls, straw thatch)
    _house(c, 2, 11, 20, 27, "wood", "wood_lt", "straw", "straw_lt",
           door=6, window=9)
    # front-right house (plaster walls, timber roof)
    _house(c, 19, 29, 21, 28, "parch", "parch_sh", "wood", "wood_sh",
           door=23, window=26)
    # tiny shed wedged between the front houses
    c.rect(13, 24, 17, 28, "wood_sh")
    c.hline(12, 18, 23, "wood")
    c.px(15, 26, "coal")
    c.outline_silhouette()
    return c


BATCH6 = [
    ("infrastructure/CapitalCity", capital_city),
    ("infrastructure/ProvinceTown", province_town),
]
