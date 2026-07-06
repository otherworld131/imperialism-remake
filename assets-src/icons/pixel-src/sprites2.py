"""Batch 2: commodity pixel sprites, 32x32."""

from pixelkit import Canvas
from sprites import thick_diag, grain, coal


def fruit():
    c = Canvas()
    c.disc(15, 18, 8, "red")
    c.disc(12, 15, 3, "red_lt"); c.px(11, 14, "red_lt")
    c.px(15, 27, "red_dk"); c.hline(12, 18, 26, "red_dk")
    c.vline(16, 7, 10, "wood_sh")
    c.line(17, 8, 22, 6, "moss"); c.px(23, 6, "moss_lt"); c.px(20, 7, "moss_lt")
    c.px(16, 11, "red_dk")  # stem dimple
    c.outline_silhouette()
    return c


def cotton():
    c = Canvas()
    # stem and branches
    c.vline(16, 14, 29, "wood_sh")
    c.line(15, 16, 10, 12, "wood_sh"); c.line(17, 16, 22, 12, "wood_sh")
    # bolls
    for cx, cy in ((10, 9), (22, 9), (16, 6)):
        c.disc(cx, cy, 4, "snow")
        c.px(cx - 2, cy - 1, "snow"); c.px(cx + 1, cy + 2, "snow_sh")
        c.px(cx, cy + 4, "wood"); c.px(cx - 1, cy + 4, "wood")  # bracts
    c.disc(16, 6, 1, "snow_sh")
    # leaf
    c.line(15, 22, 10, 19, "moss"); c.px(9, 18, "moss_lt")
    c.outline_silhouette()
    return c


def wool():
    c = Canvas()
    # fleece cloud
    c.disc(10, 18, 6, "parch"); c.disc(21, 18, 6, "parch")
    c.disc(16, 13, 6, "parch"); c.rect(6, 18, 25, 23, "parch")
    c.disc(16, 12, 4, "snow"); c.disc(9, 16, 3, "snow")
    # curl marks: little C shapes
    for cx, cy in ((11, 19), (17, 17), (22, 20), (14, 22)):
        c.px(cx - 1, cy - 1, "parch_sh"); c.px(cx, cy - 1, "parch_sh")
        c.px(cx - 1, cy, "parch_sh"); c.px(cx - 1, cy + 1, "parch_sh")
        c.px(cx, cy + 1, "parch_sh")
    c.hline(8, 23, 23, "parch_dk")
    c.outline_silhouette()
    return c


def timber():
    c = Canvas()
    for apex, base, hw in ((1, 9, 6), (6, 17, 9), (12, 25, 12)):
        c.tri(15, apex, base, hw, "pine")
        for i, y in enumerate(range(apex, base + 1)):
            w = round(hw * i / (base - apex))
            c.hline(15 - w, 15 - max(w - 2, 0), y, "pine_lt")
    c.rect(14, 26, 16, 30, "wood"); c.vline(14, 26, 30, "wood_lt")
    c.hline(9, 21, 30, "moss")
    c.outline_silhouette()
    return c


def livestock():
    c = Canvas()
    # horns
    c.hline(5, 9, 7, "parch"); c.hline(22, 26, 7, "parch")
    c.px(5, 8, "parch"); c.px(4, 9, "parch_sh"); c.px(26, 8, "parch"); c.px(27, 9, "parch_sh")
    c.vline(9, 7, 9, "parch_sh"); c.vline(22, 7, 9, "parch_sh")
    # ears
    c.rect(6, 11, 9, 14, "wood"); c.rect(22, 11, 25, 14, "wood")
    c.px(7, 12, "wood_sh"); c.px(24, 12, "wood_sh")
    # head
    c.rect(10, 8, 21, 22, "wood")
    c.rect(10, 8, 13, 22, "wood_lt")
    c.px(12, 6, "wood"); c.px(19, 6, "wood")  # poll tufts
    c.hline(12, 19, 7, "wood")
    # eyes
    c.px(12, 13, "outline"); c.px(19, 13, "outline")
    # muzzle
    c.rect(10, 23, 21, 28, "skin")
    c.hline(10, 21, 23, "skin_sh")
    c.px(13, 25, "outline"); c.px(18, 25, "outline")  # nostrils
    c.outline_silhouette()
    return c


def fish():
    c = Canvas()
    # body: horizontal ellipse
    for i, (y, x0, x1) in enumerate(((12, 12, 20), (13, 9, 23), (14, 7, 25), (15, 6, 26),
                                     (16, 6, 26), (17, 7, 25), (18, 9, 23), (19, 12, 20))):
        c.hline(x0, x1, y, "denim")
    c.hline(9, 22, 13, "denim_lt"); c.hline(7, 24, 14, "denim_lt")
    c.hline(8, 24, 18, "navy_dk")
    # tail
    c.tri(3, 12, 19, 0, "denim") if False else None
    for i, y in enumerate(range(12, 20)):
        w = abs(y - 15.5)
        c.hline(2, 2 + round(3.5 - w), y, "denim")
    # fins
    c.px(14, 10, "denim_lt"); c.px(15, 10, "denim_lt"); c.px(16, 11, "denim_lt")
    c.px(14, 21, "denim"); c.px(15, 21, "denim")
    # head details
    c.px(22, 14, "outline")  # eye
    c.line(20, 12, 20, 18, "navy_dk")  # gill
    c.px(25, 16, "navy_dk")  # mouth
    c.outline_silhouette()
    return c


def horses():
    c = Canvas()
    # neck (rising from bottom-right)
    for i in range(10):
        c.hline(14 + i, 22 + min(i, 4), 28 - i, "wood")
    c.rect(18, 20, 26, 28, "wood")
    # head angled down-left
    c.rect(8, 10, 21, 17, "wood")
    c.rect(5, 12, 9, 17, "wood")  # muzzle
    c.rect(8, 10, 21, 12, "wood_lt")
    # ears (short)
    c.rect(17, 8, 18, 9, "wood"); c.rect(20, 8, 21, 9, "wood_sh")
    # mane down the back of the neck
    for i in range(9):
        c.rect(22 + min(i // 2, 3), 11 + i * 2, 24 + min(i // 2, 3), 12 + i * 2, "wood_sh")
    c.px(15, 13, "outline")  # eye
    c.px(5, 15, "outline")   # nostril
    c.px(6, 17, "wood_sh")   # mouth shade
    c.outline_silhouette()
    return c


def iron():
    c = Canvas()
    c.disc(16, 19, 9, "grey")
    c.rect(8, 19, 24, 26, "grey")
    # facets
    c.line(10, 13, 16, 11, "grey_lt"); c.line(16, 11, 21, 14, "grey_lt")
    c.hline(9, 14, 17, "grey_lt")
    c.line(18, 17, 24, 21, "grey_dk"); c.hline(12, 22, 25, "grey_dk")
    # rust specks
    for x, y in ((12, 16), (19, 19), (15, 23), (22, 15), (9, 21)):
        c.px(x, y, "orange"); c.px(x + 1, y, "orange_lt")
    c.outline_silhouette()
    return c


def _ingot(c, x0, y, w, top, side, face):
    """Trapezoid ingot: top face lighter, front face main, end darker."""
    c.rect(x0 + 1, y, x0 + w - 2, y + 1, top)
    c.rect(x0, y + 2, x0 + w - 1, y + 6, face)
    c.rect(x0, y + 2, x0 + 1, y + 6, side)


def gold():
    c = Canvas()
    _ingot(c, 4, 19, 12, "gold_lt", "gold_sh", "gold")
    _ingot(c, 17, 19, 12, "gold_lt", "gold_sh", "gold")
    _ingot(c, 10, 12, 12, "gold_lt", "gold_sh", "gold")
    c.px(13, 14, "snow")  # glint
    c.hline(4, 28, 26, "gold_sh")
    c.outline_silhouette()
    return c


def gems():
    c = Canvas()
    # crown (top trapezoid) + table
    c.rect(11, 9, 20, 12, "teal_lt")
    c.rect(13, 9, 18, 10, "snow_sh")
    c.hline(7, 24, 13, "teal")
    c.rect(8, 14, 23, 15, "teal")
    # pavilion (triangle down)
    for i, y in enumerate(range(16, 26)):
        w = round(8 * (9 - i) / 9)
        c.hline(15 - w, 16 + w, y, "teal")
    # facet lines
    c.line(9, 14, 15, 25, "teal_dk"); c.line(22, 14, 16, 25, "teal_dk")
    c.px(12, 11, "snow"); c.px(10, 15, "teal_lt"); c.px(21, 16, "teal_dk")
    c.outline_silhouette()
    return c


def oil():
    c = Canvas()
    # drum
    c.rect(10, 7, 22, 26, "coal")
    c.hline(11, 21, 6, "coal_lt")
    c.rect(10, 7, 12, 26, "coal_lt")
    # ribs
    c.hline(10, 22, 11, "outline"); c.hline(10, 22, 21, "outline")
    # gold drop emblem
    c.px(16, 13, "gold"); c.rect(15, 14, 17, 16, "gold")
    c.rect(14, 15, 18, 17, "gold"); c.px(15, 15, "gold_lt")
    c.hline(15, 17, 18, "gold_sh")
    c.outline_silhouette()
    return c


def lumber():
    c = Canvas()
    # three stacked planks, staggered
    for x0, y in ((6, 21), (4, 15), (8, 9)):
        c.rect(x0, y, x0 + 20, y + 4, "wood")
        c.hline(x0, x0 + 20, y, "wood_lt")
        c.rect(x0 + 20, y + 1, x0 + 21, y + 4, "wood_sh")  # cut end
        c.hline(x0 + 3, x0 + 8, y + 2, "wood_sh"); c.hline(x0 + 12, x0 + 16, y + 3, "wood_sh")
    c.outline_silhouette()
    return c


def steel():
    c = Canvas()
    _ingot(c, 4, 19, 12, "steel_lt", "steel_dk", "steel")
    _ingot(c, 17, 19, 12, "steel_lt", "steel_dk", "steel")
    _ingot(c, 10, 12, 12, "steel_lt", "steel_dk", "steel")
    c.px(13, 14, "snow")
    c.hline(4, 28, 26, "steel_dk")
    c.outline_silhouette()
    return c


def fabric():
    c = Canvas()
    # rolled bolt on top
    c.rect(5, 7, 26, 11, "red")
    c.hline(6, 25, 7, "red_lt")
    c.disc(26, 9, 2, "red_lt"); c.px(26, 9, "red_dk")  # roll end spiral
    # hanging drape with folds
    c.rect(6, 12, 24, 26, "red")
    for x in (9, 14, 19):
        c.vline(x, 12, 26, "red_dk")
        c.vline(x + 2, 12, 25, "red_lt")
    # wavy hem
    c.hline(6, 24, 26, "red_dk")
    c.px(8, 27, "red"); c.px(13, 27, "red"); c.px(18, 27, "red"); c.px(23, 27, "red")
    c.outline_silhouette()
    return c


def paper():
    c = Canvas()
    # scroll body
    c.rect(8, 8, 24, 24, "parch")
    c.rect(8, 8, 9, 24, "parch_sh")
    # rolled ends
    c.rect(6, 5, 26, 8, "parch_sh"); c.hline(7, 25, 5, "parch")
    c.px(6, 6, "parch_dk"); c.px(26, 6, "parch_dk")
    c.rect(6, 24, 26, 27, "parch_sh"); c.hline(7, 25, 27, "parch_dk")
    # text lines
    for y in (11, 14, 17, 20):
        c.hline(11, 21, y, "grey")
    c.hline(11, 16, 22, "grey") if False else None
    c.outline_silhouette()
    return c


def cannedfood():
    c = Canvas()
    # can body
    c.rect(10, 7, 22, 26, "steel")
    c.rect(10, 7, 12, 26, "steel_lt")
    c.hline(10, 22, 8, "steel_lt"); c.hline(11, 21, 6, "steel_lt")  # lid
    c.hline(10, 22, 26, "steel_dk")
    # red label band
    c.rect(10, 13, 22, 20, "red")
    c.rect(10, 13, 12, 20, "red_lt")
    c.rect(14, 15, 18, 18, "parch")  # label patch
    c.hline(15, 17, 16, "grey")
    c.outline_silhouette()
    return c


def arms():
    c = Canvas()
    # barrel pointing left
    c.rect(3, 11, 20, 13, "steel")
    c.hline(4, 19, 11, "steel_lt")
    c.px(3, 10, "steel_lt")  # muzzle lip
    # lock + flint hammer
    c.rect(18, 9, 21, 11, "gold"); c.px(19, 8, "gold_sh")
    # stock curving down-right
    for i in range(9):
        c.rect(20 + i // 2, 14 + i, 25 + i // 3, 15 + i, "wood")
    c.rect(24, 21, 28, 26, "wood")
    c.px(21, 14, "wood_lt"); c.px(22, 16, "wood_lt")
    # trigger guard
    c.px(17, 15, "gold"); c.px(17, 16, "gold"); c.px(18, 17, "gold")
    c.outline_silhouette()
    return c


def furniture():
    c = Canvas()
    # side-view chair, facing left
    c.vline(21, 4, 27, "wood"); c.vline(22, 4, 27, "wood_lt")   # back post
    c.rect(21, 5, 23, 7, "wood")                                 # back top rail
    c.rect(6, 16, 22, 18, "wood"); c.hline(6, 22, 16, "wood_lt")  # seat
    c.vline(7, 19, 27, "wood"); c.vline(8, 19, 27, "wood_sh")     # front leg
    c.hline(8, 21, 23, "wood_sh")                                 # stretcher
    c.rect(20, 8, 22, 10, "wood_sh")                              # back slat
    c.outline_silhouette()
    return c


def clothing():
    c = Canvas()
    # frock coat, front view
    c.rect(9, 6, 22, 10, "navy")            # shoulders
    c.rect(8, 10, 23, 27, "navy")           # body
    c.rect(8, 10, 10, 27, "navy_lt")        # lit side
    # sleeves hinted at the sides
    c.vline(8, 7, 26, "navy_dk"); c.vline(23, 7, 26, "navy_dk")
    # collar
    c.px(14, 6, "parch"); c.px(17, 6, "parch")
    c.line(13, 7, 15, 9, "navy_lt"); c.line(18, 7, 16, 9, "navy_dk")
    # center split + brass buttons
    c.vline(16, 9, 27, "navy_dk")
    for y in (11, 15, 19, 23):
        c.px(14, y, "gold")
    # coat tails split
    c.px(15, 27, "navy_dk"); c.px(17, 27, "navy_dk")
    c.outline_silhouette()
    return c


def hardware():
    c = Canvas()
    # gear teeth (8)
    for dx, dy in ((0, -10), (0, 10), (-10, 0), (10, 0), (-7, -7), (7, -7), (-7, 7), (7, 7)):
        c.rect(15 + dx - 1, 15 + dy - 1, 16 + dx + 1, 16 + dy + 1, "steel_dk")
    # body
    c.disc(15, 15, 8, "steel")
    c.disc(13, 13, 3, "steel_lt")
    c.disc(15, 15, 3, "steel_dk")
    c.disc(15, 15, 1, None) if False else None
    # hub hole
    c.px(15, 15, "outline"); c.px(16, 15, "outline"); c.px(15, 16, "outline"); c.px(16, 16, "outline")
    c.outline_silhouette()
    return c


BATCH2 = [
    ("commodities/Grain", grain),
    ("commodities/Fruit", fruit),
    ("commodities/Cotton", cotton),
    ("commodities/Wool", wool),
    ("commodities/Timber", timber),
    ("commodities/Livestock", livestock),
    ("commodities/Fish", fish),
    ("commodities/Horses", horses),
    ("commodities/Coal", coal),
    ("commodities/Iron", iron),
    ("commodities/Gold", gold),
    ("commodities/Gems", gems),
    ("commodities/Oil", oil),
    ("commodities/Lumber", lumber),
    ("commodities/Steel", steel),
    ("commodities/Fabric", fabric),
    ("commodities/Paper", paper),
    ("commodities/CannedFood", cannedfood),
    ("commodities/Arms", arms),
    ("commodities/Furniture", furniture),
    ("commodities/Clothing", clothing),
    ("commodities/Hardware", hardware),
]
