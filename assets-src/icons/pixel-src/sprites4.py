"""Batch 4: units, infrastructure, diplomacy, ui pixel sprites, 32x32."""

from pixelkit import Canvas
from sprites import thick_diag, infantry, fort, handshake, tent


def _polygon(c, pts, color):
    """Scanline fill by ray casting (even-odd)."""
    for y in range(c.size):
        for x in range(c.size):
            inside = False
            j = len(pts) - 1
            for i in range(len(pts)):
                xi, yi = pts[i]; xj, yj = pts[j]
                if (yi > y + 0.5) != (yj > y + 0.5) and \
                        x + 0.5 < (xj - xi) * (y + 0.5 - yi) / (yj - yi) + xi:
                    inside = not inside
                j = i
            if inside:
                c.px(x, y, color)


# ---- units ----

def cavalry():
    c = Canvas()
    # saber behind: clean straight blade, gold guard, wood grip
    thick_diag(c, 5, 26, 26, 5, "steel")
    c.px(27, 4, "steel_lt"); c.px(28, 3, "steel_lt"); c.px(27, 5, "steel_lt")
    c.hline(3, 8, 25, "gold"); c.px(6, 24, "gold")
    c.rect(2, 27, 4, 29, "wood_sh"); c.px(3, 28, "wood")
    # horseshoe over it: U shape opening down
    for r, col in ((9, "gold"), (6, None)):
        pass
    for y in range(6, 25):
        for x in range(4, 28):
            dx, dy = x - 16, y - 14
            d2 = dx * dx + dy * dy
            if 36 <= d2 <= 92 and y <= 22 and not (d2 <= 92 and y > 14 and abs(dx) < 6):
                c.px(x, y, "gold")
    c.hline(9, 10, 22, "gold"); c.hline(21, 22, 22, "gold")  # heel caps
    # nail holes + highlight
    c.px(12, 8, "gold_sh"); c.px(20, 8, "gold_sh"); c.px(8, 13, "gold_sh")
    c.px(24, 13, "gold_sh"); c.px(9, 18, "gold_sh"); c.px(23, 18, "gold_sh")
    c.px(13, 7, "gold_lt"); c.px(17, 6, "gold_lt")
    c.outline_silhouette()
    return c


def artillery():
    c = Canvas()
    # barrel angled up-right
    thick_diag(c, 9, 15, 26, 6, "steel")
    c.px(27, 5, "steel_lt"); c.px(27, 6, "steel_lt")  # muzzle
    c.px(8, 16, "steel_dk"); c.px(7, 17, "steel_dk")  # breech
    # carriage trail down-left
    c.line(10, 18, 4, 26, "wood_sh"); c.line(11, 18, 5, 26, "wood")
    # spoked wheel
    c.disc(15, 22, 7, "wood")
    c.disc(15, 22, 7, "wood"); c.disc(15, 22, 5, "parch_sh")
    for dx, dy in ((0, -5), (0, 5), (-5, 0), (5, 0), (-4, -4), (4, -4), (-4, 4), (4, 4)):
        c.line(15, 22, 15 + dx, 22 + dy, "wood_sh")
    c.disc(15, 22, 1, "wood_sh"); c.px(15, 22, "outline")
    c.outline_silhouette()
    return c


def special():
    c = Canvas()
    # powder keg
    c.rect(9, 10, 23, 27, "wood")
    c.rect(9, 10, 12, 27, "wood_lt")
    c.vline(16, 10, 27, "wood_sh"); c.vline(20, 10, 27, "wood_sh")
    c.hline(9, 23, 13, "steel_dk"); c.hline(9, 23, 24, "steel_dk")  # hoops
    c.hline(10, 22, 10, "wood_sh")  # top rim
    # fuse curving out the top with a lit spark
    c.line(16, 9, 19, 6, "coal"); c.line(19, 6, 22, 5, "coal")
    c.px(23, 4, "orange"); c.px(24, 3, "gold_lt"); c.px(24, 4, "orange_lt")
    c.px(23, 3, "gold")
    c.outline_silhouette()
    return c


def general():
    c = Canvas()
    # bicorne: wide crescent with upswept tips
    _polygon(c, [(3, 20), (7, 12), (16, 8), (25, 12), (29, 20), (24, 17),
                 (16, 15), (8, 17)], "coal")
    c.px(3, 19, "coal"); c.px(29, 19, "coal")
    c.hline(8, 24, 16, "coal_lt")  # sheen
    # gold trim along the lower edge
    c.line(4, 20, 15, 16, "gold_sh"); c.line(16, 16, 28, 20, "gold_sh")
    # cockade
    c.disc(16, 13, 2, "red"); c.px(16, 13, "snow")
    c.outline_silhouette()
    return c


def army():
    c = Canvas()
    # pole
    c.vline(7, 3, 29, "wood"); c.vline(8, 3, 29, "wood_sh"); c.px(7, 2, "gold")
    # swallow-tail standard
    c.rect(9, 5, 27, 16, "red")
    c.rect(9, 5, 27, 6, "red_lt")
    c.hline(9, 27, 16, "red_dk")
    # carve the swallow notch from the fly edge
    for i in range(5):
        c.hline(27 - i, 27, 10 - i if False else 10, None)
    for i in range(5):
        for x in range(23 + i, 28):
            c.px(x, 10, None)
        break
    for dy in range(-2, 3):
        depth = 5 - abs(dy) * 2
        for x in range(28 - depth, 28):
            c.px(x, 10 + dy + 0, None)
    c.px(11, 8, "gold"); c.px(11, 12, "gold")  # emblem dots
    c.outline_silhouette()
    return c


# ---- infrastructure ----

def railroad():
    # Straight track segment matching the edge-link map art (card #497):
    # ballast bed, four sleepers, two steel rails running left to right.
    c = Canvas()
    c.rect(2, 10, 29, 21, "grey")
    for x, col in ((4, "grey_dk"), (11, "grey_lt"), (18, "grey_dk"), (25, "grey_lt")):
        c.px(x, 11, col); c.px(x + 2, 20, col)
    for tx in (3, 10, 17, 24):
        c.rect(tx, 11, tx + 2, 20, "wood")
        c.vline(tx, 11, 20, "wood_lt")
        c.vline(tx + 2, 11, 20, "wood_sh")
    for ry in (13, 18):
        c.hline(2, 29, ry, "steel_lt")
        c.hline(2, 29, ry + 1, "steel_dk")
        for tx in (3, 10, 17, 24):
            c.px(tx + 1, ry, "steel")
    c.outline_silhouette()
    return c


def depot():
    c = Canvas()
    # track under the building
    c.hline(2, 29, 28, "steel"); c.hline(2, 29, 26, "steel")
    for x in range(3, 29, 4):
        c.vline(x, 26, 28, "wood_sh")
    # warehouse
    c.rect(6, 13, 26, 24, "parch_sh")
    c.rect(6, 13, 8, 24, "parch")
    # red gable roof
    for i in range(4):
        c.hline(4 + i * 2, 27 - i * 2, 12 - i, "red")
    c.hline(6, 25, 12, "red_lt") if False else None
    c.hline(4, 27, 12, "red_dk")
    # sliding door + window
    c.rect(13, 17, 19, 24, "wood"); c.vline(16, 17, 24, "wood_sh")
    c.rect(22, 15, 24, 17, "navy_lt")
    c.rect(8, 15, 10, 17, "navy_lt")
    c.outline_silhouette()
    return c


def port():
    c = Canvas()
    # water
    c.rect(2, 25, 30, 28, "navy")
    c.hline(4, 10, 25, "navy_lt"); c.hline(16, 24, 26, "navy_lt")
    # stone quay at left
    c.rect(2, 20, 12, 24, "grey_lt")
    c.hline(2, 12, 20, "snow_sh")
    c.px(5, 22, "grey"); c.px(9, 21, "grey")
    # crane mast + jib
    c.vline(5, 6, 19, "wood"); c.vline(6, 6, 19, "wood_sh")
    c.line(6, 8, 22, 12, "wood")
    c.line(6, 12, 14, 10, "wood_sh")  # stay
    # cable + crate over the water
    c.vline(21, 13, 17, "coal")
    c.rect(18, 18, 24, 23, "wood")
    c.hline(18, 24, 18, "wood_lt"); c.vline(21, 18, 23, "wood_sh")
    c.outline_silhouette()
    return c


def capital():
    c = Canvas()
    _polygon(c, [(16, 2), (20, 11), (30, 12), (22, 18), (25, 29), (16, 23),
                 (7, 29), (10, 18), (2, 12), (12, 11)], "gold")
    # facet shading: left arm lit, lower-right shaded
    c.line(16, 4, 16, 21, "gold_lt"); c.line(15, 6, 15, 20, "gold_lt")
    c.px(23, 17, "gold_sh"); c.px(23, 26, "gold_sh"); c.px(24, 27, "gold_sh")
    c.px(21, 14, "gold_sh")
    c.outline_silhouette()
    return c


def capitol():
    c = Canvas()
    # steps
    c.rect(4, 26, 27, 28, "grey_lt"); c.hline(4, 27, 26, "snow_sh")
    # colonnade
    c.rect(6, 15, 25, 25, "parch_sh")
    for x in (8, 12, 16, 20, 23):
        c.vline(x, 16, 25, "parch"); c.vline(x + 1, 16, 25, "parch_dk")
    # entablature + pediment
    c.rect(5, 13, 26, 15, "parch")
    # dome on a drum
    c.rect(11, 10, 20, 12, "parch_sh")
    c.disc(15, 9, 5, "parch")
    c.rect(10, 9, 21, 9, "parch") if False else None
    c.hline(12, 15, 5, "snow_sh")  # dome highlight
    c.px(15, 2, "gold"); c.px(15, 3, "gold_sh")  # finial
    c.outline_silhouette()
    return c


# ---- diplomacy ----

def consulate():
    c = Canvas()
    # small house
    c.rect(8, 15, 24, 27, "parch_sh")
    c.rect(8, 15, 10, 27, "parch")
    # roof
    for i in range(5):
        c.hline(6 + i * 2, 26 - i * 2, 14 - i, "wood")
    c.hline(6, 26, 14, "wood_sh")
    # door + window
    c.rect(14, 20, 18, 27, "wood_sh"); c.px(17, 24, "gold")
    c.rect(20, 18, 22, 20, "navy_lt")
    # pennant above the roof
    c.vline(16, 4, 9, "wood_sh")
    c.rect(17, 4, 21, 5, "red"); c.px(21, 4, "red_lt")
    c.outline_silhouette()
    return c


def embassy():
    c = Canvas()
    # stately columned building
    c.rect(5, 12, 27, 26, "parch_sh")
    c.rect(5, 12, 7, 26, "parch")
    c.hline(5, 27, 12, "parch")
    c.rect(4, 10, 28, 12, "parch")  # cornice
    for x in (8, 13, 18, 23):
        c.vline(x, 14, 25, "parch"); c.vline(x + 1, 14, 25, "parch_dk")
    c.rect(14, 21, 18, 26, "wood_sh")  # door
    c.hline(4, 28, 27, "grey_lt")  # base
    # rooftop flag
    c.vline(9, 2, 9, "wood_sh")
    c.rect(10, 2, 16, 5, "red"); c.hline(10, 16, 5, "red_dk")
    c.outline_silhouette()
    return c


def alliance():
    c = Canvas()
    # two crossed poles
    thick_diag(c, 6, 6, 25, 28, "wood_sh")
    thick_diag(c, 24, 6, 5, 28, "wood_sh")
    # left flag (red) on the left pole top
    c.rect(1, 5, 12, 12, "red")
    c.rect(1, 5, 12, 6, "red_lt"); c.hline(1, 12, 12, "red_dk")
    # right flag (navy) on the right pole top
    c.rect(19, 5, 30, 12, "navy")
    c.rect(19, 5, 30, 6, "navy_lt"); c.hline(19, 30, 12, "navy_dk")
    c.px(6, 5, "gold"); c.px(25, 5, "gold")  # finials
    c.outline_silhouette()
    return c


def war():
    c = Canvas()
    # torch handle
    thick_diag(c, 14, 15, 15, 28, "wood")
    c.px(14, 28, "wood_sh"); c.px(15, 28, "wood_sh")
    # cup
    c.rect(12, 12, 18, 14, "gold"); c.hline(12, 18, 12, "gold_lt")
    # flame: layered teardrop
    _polygon(c, [(15, 2), (20, 7), (19, 12), (11, 12), (10, 7)], "orange")
    _polygon(c, [(15, 5), (18, 8), (17, 12), (13, 12), (12, 8)], "gold")
    c.px(15, 9, "gold_lt"); c.px(15, 10, "gold_lt")
    c.px(11, 5, "red_lt"); c.px(19, 4, "red_lt")  # sparks
    c.outline_silhouette()
    return c


def peace():
    c = Canvas()
    # dove flying left: body, raised wing, tail right
    c.disc(14, 18, 5, "snow")
    c.rect(14, 15, 22, 21, "snow")
    # tail
    _polygon(c, [(22, 16), (29, 13), (28, 19), (22, 20)], "snow_sh")
    # raised wing
    _polygon(c, [(13, 5), (20, 8), (16, 15), (12, 14)], "snow")
    c.line(14, 8, 15, 13, "snow_sh")
    # head + beak
    c.disc(9, 14, 3, "snow")
    c.px(8, 13, "outline")  # eye
    c.px(5, 14, "gold"); c.px(4, 14, "gold_sh")  # beak
    # olive sprig held in beak
    c.line(4, 15, 1, 17, "moss")
    c.px(2, 15, "moss_lt"); c.px(1, 18, "moss_lt"); c.px(3, 17, "moss_dk")
    c.hline(11, 18, 22, "snow_sh")  # belly shade
    c.outline_silhouette()
    return c


def grant():
    c = Canvas()
    # tied money sack
    _polygon(c, [(15, 8), (19, 10), (23, 15), (24, 22), (21, 27), (9, 27),
                 (6, 22), (7, 15), (11, 10)], "parch_sh")
    c.rect(12, 8, 18, 10, "parch_dk")  # gathered neck
    c.hline(11, 19, 10, "wood")        # tie cord
    c.px(13, 7, "parch_sh"); c.px(16, 6, "parch_sh"); c.px(19, 7, "parch_sh")  # cloth ears
    # fold lines + currency mark
    c.line(11, 14, 10, 24, "parch_dk"); c.line(20, 14, 21, 24, "parch_dk")
    c.rect(14, 16, 16, 21, "gold_sh"); c.hline(13, 17, 18, "gold_sh")
    # gold coin leaning on the sack
    c.disc(25, 23, 4, "gold")
    c.disc(25, 23, 4, "gold"); c.px(24, 21, "gold_lt")
    c.px(25, 23, "gold_sh"); c.px(26, 24, "gold_sh")
    c.outline_silhouette()
    return c


def break_treaty():
    c = Canvas()
    # left torn half (tilted out)
    _polygon(c, [(5, 6), (14, 5), (13, 9), (15, 13), (12, 17), (14, 21), (12, 26), (4, 25)],
             "parch")
    # right torn half
    _polygon(c, [(17, 5), (27, 6), (28, 25), (18, 26), (16, 21), (18, 17), (15, 13), (17, 9)],
             "parch_sh")
    # text lines on both halves
    for y in (9, 12, 15, 18):
        c.hline(7, 11, y, "grey")
        c.hline(20, 25, y, "grey")
    # wax seal on the right half
    c.disc(23, 22, 2, "red"); c.px(23, 22, "red_dk")
    c.outline_silhouette()
    return c


# ---- ui ----

def anchor():
    c = Canvas()
    # ring
    for dx, dy in ((-2, 0), (2, 0), (0, -2), (0, 2), (-1, -1), (1, -1), (-1, 1), (1, 1)):
        c.px(16 + dx, 4 + dy, "navy_dk")
    # shank
    c.rect(15, 6, 16, 21, "navy_dk"); c.vline(15, 7, 20, "navy")
    # stock
    c.rect(9, 8, 22, 9, "navy_dk"); c.hline(10, 21, 8, "navy")
    # curved arms
    for x, y in ((8, 17), (7, 18), (6, 19), (6, 20), (7, 21), (8, 22), (10, 23), (12, 24),
                 (14, 24)):
        c.px(x, y, "navy_dk"); c.px(x + 1, y, "navy_dk")
        c.px(31 - x, y, "navy_dk"); c.px(31 - x - 1, y, "navy_dk")
    c.rect(14, 24, 17, 25, "navy_dk")
    # fluke tips
    c.px(5, 17, "navy_dk"); c.px(4, 18, "navy_dk"); c.px(26, 17, "navy_dk")
    c.px(27, 18, "navy_dk")
    c.px(16, 22, "navy")
    c.outline_silhouette()
    return c


def swords():
    c = Canvas()
    # two crossed straight swords, hilts at the bottom
    thick_diag(c, 7, 25, 24, 4, "steel")
    c.px(25, 3, "steel_lt")
    thick_diag(c, 23, 25, 6, 4, "steel")
    c.px(6, 3, "steel_lt")
    # guards
    c.hline(6, 11, 24, "gold"); c.hline(20, 25, 24, "gold")
    # grips
    c.px(5, 27, "wood_sh"); c.px(4, 28, "wood_sh"); c.px(5, 28, "wood")
    c.px(26, 27, "wood_sh"); c.px(27, 28, "wood_sh"); c.px(26, 28, "wood")
    c.outline_silhouette()
    return c


def treasury():
    c = Canvas()
    c.disc(16, 16, 10, "gold")
    # rim
    for r_out, col in ((10, "gold_sh"),):
        pass
    c.disc(16, 16, 8, "gold_lt"); c.disc(16, 16, 7, "gold")
    # crown emboss
    c.rect(11, 17, 21, 20, "gold_sh")
    _polygon(c, [(11, 17), (11, 12), (13, 15), (16, 11), (19, 15), (21, 12), (21, 17)],
             "gold_sh")
    c.px(11, 11, "gold_lt"); c.px(16, 10, "gold_lt"); c.px(21, 11, "gold_lt")
    c.hline(12, 20, 19, "gold")
    c.px(9, 12, "snow")  # glint
    c.outline_silhouette()
    return c


def workers():
    c = Canvas()
    # back bust (right), moss cap
    c.rect(19, 16, 28, 22, "moss_dk")            # shoulders
    c.rect(21, 8, 26, 15, "skin_sh")             # head
    c.rect(20, 6, 27, 8, "moss"); c.hline(19, 27, 8, "moss_dk")  # cap
    c.px(22, 11, "outline"); c.px(25, 11, "outline")
    # front bust (left, overlapping), navy cap
    c.rect(3, 18, 15, 26, "navy")                # shoulders
    c.hline(3, 15, 18, "navy_lt")
    c.rect(6, 9, 12, 17, "skin")                 # head
    c.rect(5, 7, 13, 9, "denim"); c.hline(4, 13, 9, "denim_lt")  # cap w/ brim
    c.px(7, 12, "outline"); c.px(11, 12, "outline")
    c.outline_silhouette()
    return c


def freightcar():
    c = Canvas()
    # rail
    c.hline(2, 29, 28, "steel")
    # boxcar body
    c.rect(4, 9, 27, 23, "brickish" if False else "red_dk")
    c.rect(4, 9, 6, 23, "red")
    c.hline(4, 27, 9, "red")
    for x in (9, 22):  # plank seams
        c.vline(x, 10, 23, "outline")
    # sliding door
    c.rect(12, 12, 19, 23, "wood")
    c.vline(15, 12, 23, "wood_sh"); c.hline(12, 19, 12, "wood_lt")
    # roof
    c.rect(3, 7, 28, 8, "coal"); c.hline(4, 27, 7, "coal_lt")
    # wheels
    c.disc(9, 26, 2, "coal"); c.disc(22, 26, 2, "coal")
    c.px(9, 26, "steel"); c.px(22, 26, "steel")
    c.outline_silhouette()
    return c


def science():
    c = Canvas()
    # erlenmeyer flask: neck then conical body
    c.rect(13, 3, 18, 4, "steel_lt")  # lip
    c.vline(13, 5, 12, "steel_lt"); c.vline(18, 5, 12, "steel_lt")
    for i in range(14):  # cone sides
        y = 13 + i
        w = 3 + round(i * 0.65)
        c.px(15 - w, y, "steel_lt"); c.px(16 + w, y, "steel_lt")
    c.hline(3, 28, 27, "steel_lt")  # base
    # teal liquid in the lower half
    for i in range(7, 14):
        y = 13 + i
        w = 3 + round(i * 0.65) - 1
        c.hline(15 - w, 16 + w, y, "teal")
    c.hline(9, 22, 20, "teal_lt")
    # bubbles
    c.px(13, 17, "teal_lt"); c.px(17, 15, "teal_lt"); c.px(15, 11, "snow_sh")
    c.outline_silhouette()
    return c


def worker_untrained():
    c = Canvas()
    # bare-headed labourer bust: scruffy hair, plain denim shirt
    c.rect(7, 21, 24, 29, "denim")
    c.hline(7, 24, 21, "denim_lt")
    c.rect(11, 8, 20, 20, "skin")
    c.rect(11, 6, 20, 9, "wood_sh")
    c.px(10, 8, "wood_sh"); c.px(21, 8, "wood_sh")
    c.px(13, 13, "outline"); c.px(18, 13, "outline")
    c.hline(14, 17, 17, "skin_sh")
    c.outline_silhouette()
    return c


def worker_trained():
    c = Canvas()
    # flat-capped worker bust, navy work shirt
    c.rect(7, 21, 24, 29, "navy")
    c.hline(7, 24, 21, "navy_lt")
    c.rect(11, 10, 20, 20, "skin")
    c.rect(10, 7, 21, 10, "denim")
    c.hline(9, 23, 10, "denim_lt")
    c.px(13, 14, "outline"); c.px(18, 14, "outline")
    c.hline(14, 17, 18, "skin_sh")
    c.outline_silhouette()
    return c


def worker_expert():
    c = Canvas()
    # top-hatted foreman bust, dark suit with gold pin
    c.rect(7, 21, 24, 29, "coal")
    c.hline(7, 24, 21, "coal_lt")
    c.px(16, 24, "gold")
    c.rect(11, 12, 20, 20, "skin")
    c.rect(11, 2, 20, 11, "coal")
    c.hline(11, 20, 9, "gold")
    c.hline(8, 23, 11, "coal_lt")
    c.px(13, 15, "outline"); c.px(18, 15, "outline")
    c.hline(14, 17, 19, "skin_sh")
    c.outline_silhouette()
    return c


def factory():
    c = Canvas()
    # smoke puffs drifting off the chimney
    c.px(25, 2, "snow_sh"); c.px(27, 3, "snow_sh"); c.px(24, 4, "snow_sh")
    # sawtooth roof: three teeth slanting up-right, vertical drop
    for x0 in (3, 10, 17):
        for i in range(7):
            c.vline(x0 + i, 14 - i, 14, "navy")
            c.px(x0 + i, 14 - i, "navy_lt")
    # chimney in front of the rightmost tooth
    c.rect(23, 5, 26, 14, "red_dk"); c.vline(23, 5, 14, "red")
    c.hline(22, 27, 5, "coal")
    # brick hall
    c.rect(3, 15, 28, 26, "red_dk")
    c.rect(3, 15, 4, 26, "red")
    c.hline(3, 28, 15, "red")
    # lit windows + door
    for x in (7, 13, 24):
        c.rect(x, 18, x + 2, 21, "gold")
        c.px(x, 18, "gold_lt")
    c.rect(18, 20, 21, 26, "coal")
    c.vline(19, 20, 26, "coal_lt")
    # ground
    c.hline(2, 29, 27, "grey_dk")
    c.outline_silhouette()
    return c


def warehouse():
    c = Canvas()

    def crate(x0, y0, x1, y1):
        c.rect(x0, y0, x1, y1, "wood")
        c.frame(x0, y0, x1, y1, "wood_sh")
        c.hline(x0, x1, y0, "wood_lt"); c.vline(x0, y0, y1, "wood_lt")
        # X brace
        span = x1 - x0
        for i in range(span + 1):
            y = y0 + round(i * (y1 - y0) / span)
            c.px(x0 + i, y, "wood_sh")
            c.px(x1 - i, y, "wood_sh")

    crate(4, 17, 15, 28)
    crate(17, 17, 28, 28)
    crate(10, 5, 21, 16)
    c.hline(2, 29, 29, "grey_dk")
    c.outline_silhouette()
    return c


def news():
    c = Canvas()
    # folded paper: top fold face + front face
    _polygon(c, [(4, 12), (10, 6), (28, 6), (22, 12)], "snow")
    c.rect(4, 12, 22, 26, "parch")
    c.rect(23, 12, 27, 25, "parch_sh") if False else None
    _polygon(c, [(23, 12), (28, 7), (28, 20), (23, 26)], "parch_sh")
    # masthead + text
    c.rect(6, 14, 15, 16, "coal")
    for y in (19, 21, 23):
        c.hline(6, 19, y, "grey")
    c.hline(6, 13, 25, "grey")
    c.outline_silhouette()
    return c


BATCH4 = [
    ("units/Infantry", infantry),
    ("units/Cavalry", cavalry),
    ("units/Artillery", artillery),
    ("units/Special", special),
    ("units/General", general),
    ("units/Army", army),
    ("infrastructure/Railroad", railroad),
    ("infrastructure/Depot", depot),
    ("infrastructure/Port", port),
    ("infrastructure/Fort", fort),
    ("infrastructure/Capital", capital),
    ("infrastructure/Capitol", capitol),
    ("diplomacy/Consulate", consulate),
    ("diplomacy/Embassy", embassy),
    ("diplomacy/NonAggressionPact", handshake),
    ("diplomacy/Alliance", alliance),
    ("diplomacy/War", war),
    ("diplomacy/Peace", peace),
    ("diplomacy/Grant", grant),
    ("diplomacy/BreakTreaty", break_treaty),
    ("ui/Anchor", anchor),
    ("ui/Swords", swords),
    ("ui/Treasury", treasury),
    ("ui/Workers", workers),
    ("ui/FreightCar", freightcar),
    ("ui/Science", science),
    ("ui/News", news),
    ("ui/Tent", tent),
    ("ui/Factory", factory),
    ("ui/Warehouse", warehouse),
    ("ui/WorkerUntrained", worker_untrained),
    ("ui/WorkerTrained", worker_trained),
    ("ui/WorkerExpert", worker_expert),
]
