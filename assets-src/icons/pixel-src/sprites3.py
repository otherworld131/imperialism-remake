"""Batch 3: ship pixel sprites, 32x32. Side views, bow to the right."""

from pixelkit import Canvas
from sprites import frigate, ironclad


def _waterline(c, y=26):
    c.hline(1, 30, y, "navy")
    c.hline(3, 8, y + 1, "navy_lt"); c.hline(13, 19, y + 1, "navy_lt")
    c.hline(23, 29, y + 1, "navy_lt")


def _sq_sails(c, x, top, w=3, rows=((0, 5), (6, 10))):
    """Stacked square sails centered on mast x."""
    for y0, y1 in rows:
        c.rect(x - w, top + y0, x + w, top + y1, "parch")
        c.vline(x - w, top + y0, top + y1, "parch_dk")
        c.hline(x - w + 1, x + w, top + y0, "parch_sh") if False else None


def trader():
    c = Canvas()
    # small hull
    c.rect(7, 20, 24, 24, "wood")
    c.hline(8, 25, 25, "wood_sh")
    c.hline(7, 24, 20, "wood_lt")
    c.line(25, 20, 27, 18, "wood")  # bow rise
    # single mast + fore-and-aft sail
    c.vline(15, 3, 19, "wood_sh")
    for i in range(10):  # triangular mainsail behind mast
        c.hline(15 - min(i, 7), 14, 6 + i, "parch")
    c.vline(8, 12, 15, "parch_dk")
    for i in range(8):  # jib toward bow
        c.hline(17, 17 + min(i, 6), 8 + i, "parch_sh")
    c.rect(16, 3, 18, 3, "red")  # pennant
    _waterline(c)
    c.outline_silhouette()
    return c


def indiaman():
    c = Canvas()
    # broad hull
    c.rect(3, 20, 27, 25, "wood")
    c.hline(3, 27, 20, "wood_lt")
    c.hline(4, 26, 26, "wood_sh")
    c.hline(5, 25, 21, "gold_sh")  # rail stripe
    c.line(28, 20, 30, 18, "wood")
    # three masts, stacked square sails
    for x in (8, 15, 22):
        c.vline(x, 4, 19, "wood_sh")
        _sq_sails(c, x, 5, 3, ((0, 4), (5, 9), (10, 13)))
    c.rect(16, 2, 19, 2, "red")
    _waterline(c)
    c.outline_silhouette()
    return c


def clipper():
    c = Canvas()
    # sleek dark hull, sharp bow
    c.rect(2, 21, 26, 24, "coal")
    c.hline(2, 26, 21, "coal_lt")
    c.line(27, 21, 30, 19, "coal")
    c.hline(3, 25, 25, "outline")
    c.hline(4, 25, 22, "gold_sh")  # gold sheer line
    # three raked masts (leaning aft)
    for xb in (7, 14, 21):
        c.line(xb, 20, xb + 2, 4, "wood_sh")
        # tall narrow sail stack on each
        for y0, y1, off in ((5, 9, 2), (10, 14, 1), (15, 19, 0)):
            c.rect(xb + off - 2, y0, xb + off + 2, y1, "parch")
            c.vline(xb + off - 2, y0, y1, "parch_dk")
    # stay sail at bow
    for i in range(6):
        c.hline(24 + min(i, 4), 28, 13 + i, "parch_sh")
    c.rect(24, 2, 27, 2, "red")
    _waterline(c)
    c.outline_silhouette()
    return c


def paddlewheeler():
    c = Canvas()
    # hull
    c.rect(3, 20, 28, 24, "wood")
    c.hline(3, 28, 20, "wood_lt"); c.hline(4, 27, 25, "wood_sh")
    # deckhouse
    c.rect(6, 15, 14, 19, "parch_sh"); c.hline(6, 14, 15, "parch")
    c.px(8, 17, "coal"); c.px(12, 17, "coal")  # windows
    # funnel + smoke
    c.rect(17, 7, 19, 14, "coal"); c.hline(17, 19, 7, "coal_lt")
    c.rect(21, 4, 22, 5, "steel_lt"); c.px(24, 3, "steel_lt")
    # side paddle wheel: red housing ring, lighter interior, dark spokes
    c.disc(23, 19, 6, "red_dk")
    c.disc(23, 19, 4, "parch_sh")
    for dx, dy in ((0, -4), (0, 4), (-4, 0), (4, 0), (-3, -3), (3, -3), (-3, 3), (3, 3)):
        c.line(23, 19, 23 + dx, 19 + dy, "wood_sh")
    c.px(23, 19, "outline")
    _waterline(c)
    c.outline_silhouette()
    return c


def freighter():
    c = Canvas()
    # steel hull
    c.rect(2, 19, 29, 25, "steel_dk")
    c.hline(2, 29, 19, "steel")
    c.hline(3, 28, 24, "outline"); c.hline(3, 28, 25, "red_dk")  # boot top
    # central funnel + deckhouse
    c.rect(13, 13, 18, 18, "parch_sh")
    c.rect(14, 6, 16, 12, "coal"); c.hline(14, 16, 6, "gold_sh")
    c.px(18, 4, "steel_lt"); c.px(20, 3, "steel_lt")
    # derrick masts with booms
    c.vline(7, 8, 18, "wood_sh"); c.line(7, 12, 11, 16, "wood")
    c.vline(24, 8, 18, "wood_sh"); c.line(24, 12, 20, 16, "wood")
    _waterline(c)
    c.outline_silhouette()
    return c


def ship_of_the_line():
    c = Canvas()
    # tall hull, two gunport rows
    c.rect(3, 17, 27, 25, "navy_dk")
    c.hline(3, 27, 17, "navy")
    c.hline(4, 26, 19, "gold_sh"); c.hline(4, 26, 22, "gold_sh")
    for x in (6, 10, 14, 18, 22, 25):
        c.px(x, 20, "coal"); c.px(x + 1, 23, "coal")
    c.line(28, 17, 30, 15, "navy_dk")
    # three masts, full sails
    for x in (8, 15, 22):
        c.vline(x, 3, 16, "wood_sh")
        _sq_sails(c, x, 4, 3, ((0, 4), (5, 9)))
    c.rect(16, 1, 19, 1, "red")
    _waterline(c)
    c.outline_silhouette()
    return c


def raider():
    c = Canvas()
    # low steam hull with ram bow
    c.rect(3, 20, 26, 24, "coal")
    c.hline(3, 26, 20, "coal_lt")
    c.line(27, 20, 30, 22, "coal"); c.px(30, 23, "coal")  # ram nose dipping
    # single gun forward
    c.rect(19, 17, 25, 18, "steel_dk")
    c.rect(17, 16, 20, 19, "steel")
    # funnel raked + mast
    c.line(12, 13, 13, 19, "coal"); c.line(13, 13, 14, 19, "coal")
    c.hline(12, 13, 12, "coal_lt")
    c.px(10, 10, "steel_lt"); c.px(8, 9, "steel_lt")
    c.vline(7, 12, 19, "wood_sh")
    _waterline(c)
    c.outline_silhouette()
    return c


def advanced_ironclad():
    c = Canvas()
    # hull with gold stripe
    c.rect(2, 19, 29, 24, "coal")
    c.hline(2, 29, 19, "coal_lt")
    c.hline(3, 28, 21, "gold_sh")
    # long deckhouse
    c.rect(8, 14, 22, 18, "steel")
    c.rect(8, 14, 10, 18, "steel_lt")
    c.px(12, 16, "coal"); c.px(16, 16, "coal"); c.px(20, 16, "coal")
    # stern turret (left)
    c.rect(3, 15, 7, 18, "steel_dk")
    c.rect(1, 16, 3, 16, "steel_dk")  # gun aft
    # two gold-banded funnels
    for x in (12, 18):
        c.rect(x, 7, x + 2, 13, "coal")
        c.hline(x, x + 2, 8, "gold")
    c.px(22, 5, "steel_lt"); c.px(24, 4, "steel_lt")
    _waterline(c)
    c.outline_silhouette()
    return c


def armoured_cruiser():
    c = Canvas()
    # long steel hull
    c.rect(1, 19, 29, 24, "steel_dk")
    c.hline(1, 29, 19, "steel")
    c.hline(2, 28, 24, "outline")
    c.line(30, 19, 31, 18, "steel_dk")
    # deckhouse
    c.rect(10, 15, 21, 18, "steel")
    # two funnels + two masts
    for x in (13, 18):
        c.rect(x, 8, x + 1, 14, "coal"); c.px(x, 8, "coal_lt")
    c.vline(6, 6, 18, "wood_sh"); c.vline(25, 8, 18, "wood_sh")
    c.px(6, 5, "red")
    # bow gun
    c.rect(26, 17, 29, 18, "steel")
    _waterline(c)
    c.outline_silhouette()
    return c


def dreadnought():
    c = Canvas()
    # heavy grey hull
    c.rect(1, 18, 30, 24, "steel")
    c.hline(1, 30, 18, "steel_lt")
    c.hline(2, 29, 24, "steel_dk")
    # fore and aft turrets with long guns
    c.rect(21, 14, 26, 17, "steel_dk"); c.rect(27, 15, 31, 15, "steel_dk")
    c.rect(5, 14, 10, 17, "steel_dk"); c.rect(0, 15, 4, 15, "steel_dk")
    # central superstructure + tripod mast
    c.rect(13, 12, 18, 17, "steel_lt")
    c.vline(15, 4, 11, "steel_dk")
    c.line(13, 11, 15, 6, "steel_dk"); c.line(17, 11, 15, 6, "steel_dk")
    c.rect(14, 5, 16, 6, "steel")  # top
    # funnel
    c.rect(11, 8, 12, 11, "coal")
    _waterline(c)
    c.outline_silhouette()
    return c


def battlecruiser():
    c = Canvas()
    # sleek long hull
    c.rect(1, 20, 29, 24, "steel")
    c.hline(1, 29, 20, "steel_lt")
    c.line(30, 20, 31, 19, "steel")
    c.hline(2, 28, 24, "steel_dk")
    # low superstructure
    c.rect(8, 16, 23, 19, "steel_lt")
    # three raked funnels
    for x in (11, 15, 19):
        c.line(x, 10, x + 1, 15, "coal"); c.line(x + 1, 10, x + 2, 15, "coal")
        c.px(x, 9, "coal_lt")
    c.px(23, 7, "steel_lt"); c.px(25, 6, "steel_lt")
    # bow turret
    c.rect(24, 17, 27, 19, "steel_dk"); c.rect(28, 18, 31, 18, "steel_dk")
    # mast
    c.vline(6, 8, 15, "wood_sh"); c.px(6, 7, "red")
    _waterline(c)
    c.outline_silhouette()
    return c


BATCH3 = [
    ("ships/Trader", trader),
    ("ships/Indiaman", indiaman),
    ("ships/Clipper", clipper),
    ("ships/Paddlewheeler", paddlewheeler),
    ("ships/Freighter", freighter),
    ("ships/Frigate", frigate),
    ("ships/ShipOfTheLine", ship_of_the_line),
    ("ships/Raider", raider),
    ("ships/Ironclad", ironclad),
    ("ships/AdvancedIronclad", advanced_ironclad),
    ("ships/ArmouredCruiser", armoured_cruiser),
    ("ships/Dreadnought", dreadnought),
    ("ships/Battlecruiser", battlecruiser),
]
