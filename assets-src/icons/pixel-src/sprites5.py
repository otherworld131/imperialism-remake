"""Batch 5: top-bar screen-tab pictograms (card: icon nav rework), 32x32."""

from pixelkit import Canvas


def map_screen():
    """Folded field map: parchment panels, teal water, red dashed route."""
    c = Canvas()
    # three-panel folded sheet; middle panel dips one row (fold)
    c.rect(3, 7, 11, 25, "parch")
    c.rect(12, 8, 19, 26, "parch_sh")
    c.rect(20, 7, 28, 25, "parch")
    # fold creases
    c.vline(11, 7, 25, "parch_dk")
    c.vline(20, 7, 25, "parch_dk")
    # water region bottom-left
    for x0, x1, y in ((4, 9, 20), (4, 10, 21), (4, 11, 22), (4, 13, 23), (4, 14, 24), (4, 14, 25)):
        c.hline(x0, x1, y, "teal")
    c.hline(5, 8, 21, "teal_lt")
    # a moss landmass patch top-right
    for x0, x1, y in ((21, 27, 9), (20, 27, 10), (21, 26, 11), (22, 25, 12)):
        c.hline(x0, x1, y, "moss")
    # red dashed route with endpoint cross
    for x, y in ((7, 11), (9, 12), (11, 14), (13, 15), (15, 17), (17, 18), (19, 20), (21, 21)):
        c.px(x, y, "red")
    c.px(23, 22, "red_dk")
    c.px(24, 23, "red_dk")
    c.px(24, 21, "red_dk")
    c.px(23, 24, "red_dk")  # X marks the destination
    c.px(25, 24, "red_dk")
    c.px(25, 20, "red_dk")
    c.px(22, 25, "red_dk")
    c.px(26, 25, "red_dk")
    # start dot
    c.px(6, 10, "red_dk")
    c.px(7, 10, "red_dk")
    c.px(6, 11, "red_dk")
    c.outline_silhouette()
    return c


def diplomacy_screen():
    """Treaty in signing: document with wax seal and a gold quill."""
    c = Canvas()
    # document
    c.rect(6, 6, 22, 27, "parch")
    c.rect(7, 7, 21, 8, "parch_sh")
    # text lines
    for y in (11, 14, 17, 20):
        c.hline(9, 19, y, "grey")
    c.hline(9, 14, 23, "grey")
    # red wax seal bottom-left of the document
    c.disc(11, 24, 3, "red")
    c.disc(11, 24, 1, "red_lt")
    # quill: gold feather diagonal from top-right, nib touching the page
    quill = [(28, 5), (27, 6), (27, 7), (26, 8), (25, 9), (25, 10), (24, 11),
             (23, 12), (23, 13), (22, 14), (21, 15), (21, 16), (20, 17)]
    for x, y in quill:
        c.px(x, y, "gold")
        c.px(x + 1, y, "gold_lt")
    # feather barbs
    for x, y in ((29, 6), (28, 7), (28, 8), (27, 9), (27, 10), (26, 11)):
        c.px(x, y, "gold_sh")
    # nib
    c.px(19, 18, "coal")
    c.px(18, 19, "coal")
    c.outline_silhouette()
    return c


def trade_screen():
    """Merchant balance scales, gold beam and pans."""
    c = Canvas()
    # base + post
    c.rect(12, 26, 20, 27, "wood_sh")
    c.rect(14, 24, 18, 25, "wood")
    c.rect(15, 8, 17, 24, "wood")
    c.vline(15, 9, 23, "wood_lt")
    # beam
    c.rect(5, 7, 27, 8, "gold")
    c.hline(6, 26, 7, "gold_lt")
    # pivot
    c.rect(15, 5, 17, 7, "gold_sh")
    # hangers
    for x in (6, 26):
        c.vline(x, 9, 14, "gold_sh")
    c.px(5, 9, "gold_sh")
    c.px(7, 9, "gold_sh")
    c.px(25, 9, "gold_sh")
    c.px(27, 9, "gold_sh")
    # pans (shallow arcs)
    for cx in (6, 26):
        c.hline(cx - 4, cx + 4, 15, "gold")
        c.hline(cx - 3, cx + 3, 16, "gold_sh")
        c.hline(cx - 2, cx + 2, 17, "gold_sh")
    # goods on the left pan
    c.px(5, 14, "coal")
    c.px(6, 14, "coal_lt")
    c.px(7, 14, "coal")
    c.outline_silhouette()
    return c


def ledger_screen():
    """Open ledger book: two pages, entry lines and a red bookmark."""
    c = Canvas()
    # covers
    c.rect(3, 8, 28, 26, "wood_sh")
    # left / right pages
    c.rect(5, 9, 15, 24, "parch")
    c.rect(16, 9, 26, 24, "parch_sh")
    # spine shadow
    c.vline(15, 9, 24, "parch_dk")
    c.vline(16, 9, 24, "parch_dk")
    # entry lines: text left, figures right
    for y in (12, 15, 18, 21):
        c.hline(7, 12, y, "grey")
        c.hline(18, 22, y, "grey")
        c.px(24, y, "navy")
    # totals rule
    c.hline(18, 24, 23, "grey_dk")
    # red bookmark ribbon
    c.vline(21, 5, 9, "red")
    c.px(20, 5, "red_lt")
    c.px(21, 4, "red_lt")
    c.outline_silhouette()
    return c


def legend_screen():
    """Map key card: color chips with caption lines."""
    c = Canvas()
    # card
    c.rect(5, 5, 27, 27, "parch")
    c.rect(6, 6, 26, 7, "parch_sh")
    # title bar
    c.hline(8, 20, 9, "coal")
    # chips + caption lines
    for chip, y in (("red", 13), ("moss", 18), ("navy", 23)):
        c.rect(8, y - 1, 11, y + 1, chip)
        c.hline(14, 24, y, "grey")
    c.outline_silhouette()
    return c


BATCH5 = [
    ("ui/Map", map_screen),
    ("ui/Diplomacy", diplomacy_screen),
    ("ui/Trade", trade_screen),
    ("ui/Ledger", ledger_screen),
    ("ui/Legend", legend_screen),
]
