"""Sample-batch pixel sprites, 32x32. Each returns a Canvas."""

from pixelkit import Canvas


def thick_diag(c, x0, y0, x1, y1, color, w=2):
    """Solid diagonal bar: same color across the width (no checker dither)."""
    for i in range(w):
        c.line(x0 + i, y0, x1 + i, y1, color)


def _worker_base(c, hat, shirt="navy", arm="navy_lt", bib="wood", bib_lt="wood_lt",
                 legs="wood_sh", straps=True):
    """Shared civilian body: head cx=13, torso, legs, boots. hat(c) draws headgear."""
    # legs + boots
    c.rect(9, 24, 11, 28, legs); c.rect(15, 24, 17, 28, legs)
    c.rect(8, 29, 11, 30, "coal");    c.rect(15, 29, 18, 30, "coal")
    # torso shirt + overall bib
    c.rect(8, 14, 18, 23, shirt)
    if bib:
        c.rect(10, 17, 16, 23, bib); c.rect(10, 17, 12, 23, bib_lt)
        if straps:
            c.vline(10, 14, 16, bib); c.vline(16, 14, 16, bib)
    # arms
    c.rect(6, 15, 7, 21, arm); c.rect(19, 15, 20, 21, arm)
    c.rect(6, 22, 7, 23, "skin");    c.rect(19, 22, 20, 23, "skin")  # hands
    # head
    c.rect(10, 8, 16, 13, "skin")
    c.px(11, 13, "skin_sh"); c.px(15, 13, "skin_sh")
    c.px(11, 10, "outline"); c.px(15, 10, "outline")  # eyes
    hat(c)


def _farmer_body(c):
    def hat(c):
        c.rect(11, 3, 15, 5, "straw")
        c.rect(7, 6, 19, 7, "straw"); c.hline(8, 18, 6, "straw_lt")
        c.hline(11, 15, 5, "wood_sh")  # band
    _worker_base(c, hat, shirt="moss", arm="moss_lt")


def farmer():
    c = Canvas()
    _farmer_body(c)
    # pitchfork right of figure: steel tines + wood handle held by right hand
    c.vline(23, 9, 29, "wood"); c.px(23, 9, "wood_lt")
    c.vline(21, 4, 8, "steel"); c.vline(23, 3, 8, "steel"); c.vline(25, 4, 8, "steel")
    c.hline(21, 25, 8, "steel_dk")
    c.rect(20, 21, 23, 22, "skin")  # right hand gripping
    c.outline_silhouette()
    return c


def farmer_work():
    """Work frame: pitchfork thrust down into the ground, hay flying."""
    c = Canvas()
    _farmer_body(c)
    thick_diag(c, 19, 12, 25, 20, "wood")           # handle angled down
    c.hline(24, 30, 21, "steel_dk")                 # crossbar
    c.vline(24, 22, 27, "steel"); c.vline(27, 22, 27, "steel")
    c.vline(30, 22, 27, "steel")
    c.px(22, 29, "gold_lt"); c.px(28, 29, "gold_lt"); c.px(31, 28, "straw_lt")  # hay
    c.rect(19, 13, 21, 15, "skin")                  # hand high on the handle
    c.outline_silhouette()
    return c


def _miner_body(c):
    def hat(c):
        c.rect(9, 5, 17, 7, "gold"); c.hline(10, 16, 4, "gold")
        c.rect(12, 5, 14, 6, "gold_lt")  # lamp
        c.px(13, 5, "snow")
    _worker_base(c, hat, shirt="red", arm="red_lt", bib="coal", bib_lt="coal_lt", legs="coal")


def miner():
    c = Canvas()
    _miner_body(c)
    # pickaxe: vertical-ish handle at right, curved steel pick head on top
    thick_diag(c, 23, 22, 25, 8, "wood")
    c.px(24, 8, "wood_lt")
    # pick head: symmetric arc over the handle, both tips curving down
    c.hline(18, 30, 6, "steel"); c.hline(20, 28, 5, "steel_lt")
    c.px(17, 7, "steel"); c.px(16, 8, "steel_dk"); c.px(16, 9, "steel_dk")
    c.px(31, 7, "steel"); c.px(31, 8, "steel_dk"); c.px(31, 9, "steel_dk")
    c.rect(20, 20, 23, 22, "skin")  # hand gripping
    c.outline_silhouette()
    return c


def miner_work():
    """Work frame: pick swung down into the rock, spark at the impact."""
    c = Canvas()
    _miner_body(c)
    thick_diag(c, 19, 12, 26, 21, "wood")
    # pick head curving into the ground
    c.px(25, 20, "steel"); c.px(27, 22, "steel"); c.px(28, 23, "steel")
    c.px(29, 25, "steel"); c.px(30, 27, "steel_dk"); c.px(24, 19, "steel_dk")
    c.px(31, 28, "gold_lt"); c.px(29, 29, "gold_lt")  # sparks
    c.rect(19, 13, 21, 15, "skin")
    c.outline_silhouette()
    return c


def _engineer_body(c):
    def hat(c):
        c.rect(9, 5, 17, 7, "navy_lt"); c.rect(11, 3, 15, 4, "navy_lt")
        c.hline(11, 15, 3, "navy"); c.hline(8, 18, 7, "navy")
    _worker_base(c, hat, shirt="parch_sh", arm="parch", bib="navy", bib_lt="navy_lt",
                 legs="navy_dk")


def engineer():
    c = Canvas()
    _engineer_body(c)
    # big wrench held upright in right hand: shaft + open jaw at top
    thick_diag(c, 24, 21, 24, 9, "steel", 2)
    c.vline(24, 9, 21, "steel_lt")
    # open-end jaw (C shape opening right)
    c.rect(22, 4, 27, 5, "steel")
    c.rect(22, 8, 27, 9, "steel")
    c.rect(22, 5, 23, 8, "steel")
    c.px(22, 4, "steel_lt"); c.px(22, 5, "steel_lt")
    c.rect(20, 19, 23, 21, "skin")  # hand gripping shaft
    c.outline_silhouette()
    return c


def engineer_work():
    """Work frame: wrench swung overhead, clink sparks."""
    c = Canvas()
    _engineer_body(c)
    thick_diag(c, 21, 17, 26, 10, "steel")
    c.px(22, 16, "steel_lt")
    # jaw at the top of the swing
    c.rect(25, 4, 30, 5, "steel")
    c.rect(25, 8, 30, 9, "steel")
    c.rect(25, 5, 26, 8, "steel")
    c.px(25, 4, "steel_lt")
    c.px(31, 2, "gold_lt"); c.px(29, 1, "gold_lt")  # clink sparks
    c.rect(20, 17, 22, 19, "skin")
    c.outline_silhouette()
    return c


def mountain():
    c = Canvas()
    # back peak (right, darker)
    c.tri(22, 8, 28, 9, "grey_dk")
    c.tri(22, 8, 13, 4, "snow_sh")
    # front peak
    c.tri(11, 3, 28, 12, "grey")
    for i, y in enumerate(range(3, 10)):  # left-lit face
        c.hline(11 - round(12 * i / 25), 11, y, "grey_lt")
    c.tri(11, 3, 8, 3, "snow")  # snowcap
    c.px(9, 9, "snow"); c.px(13, 9, "snow"); c.px(11, 10, "snow_sh")  # jag
    # soft shaded right face instead of a hard crack line
    for i, y in enumerate(range(10, 28)):
        w = round(12 * (y - 3) / 25)
        c.hline(11 + max(w - 3, 1), 11 + w, y, "grey_dk")
    c.outline_silhouette()
    return c


def forest():
    c = Canvas()
    # back pine (right, small)
    for i, (apex, base, hw) in enumerate(((6, 12, 4), (10, 17, 6), (14, 22, 7))):
        c.tri(23, apex, base, hw, "pine_dk")
    c.rect(22, 23, 24, 26, "wood_sh")
    # front pine (left, tall)
    for apex, base, hw in ((2, 9, 5), (7, 16, 7), (12, 24, 9)):
        c.tri(11, apex, base, hw, "pine")
        c.tri(11, apex, base, 2, "pine_lt") if False else None
    for apex, base, hw in ((2, 9, 5), (7, 16, 7), (12, 24, 9)):
        for i, y in enumerate(range(apex, base + 1)):
            w = round(hw * i / (base - apex))
            c.hline(11 - w, 11 - max(w - 2, 0), y, "pine_lt")
    c.rect(10, 25, 12, 29, "wood"); c.vline(10, 25, 29, "wood_lt")
    # ground tufts
    c.hline(4, 8, 29, "moss"); c.hline(18, 27, 29, "moss")
    c.outline_silhouette()
    return c


def swamp():
    c = Canvas()
    # wide flat pool (elliptical ends)
    c.rect(3, 21, 28, 26, "murk")
    c.hline(5, 26, 20, "murk"); c.hline(5, 26, 27, "murk_dk")
    c.hline(2, 3, 23, "murk"); c.hline(28, 29, 23, "murk")
    c.hline(6, 13, 22, "murk_lt"); c.hline(18, 25, 24, "murk_lt")  # sheen
    # two cattail reeds, thin
    for x, top in ((10, 6), (21, 9)):
        c.vline(x, top + 5, 21, "pine_dk")
        c.rect(x, top, x + 1, top + 4, "wood")
        c.px(x, top, "wood_lt")
    # bent grass blades
    c.line(15, 20, 14, 15, "moss"); c.px(13, 14, "moss_lt")
    c.line(25, 20, 26, 16, "moss"); c.px(27, 15, "moss_lt")
    c.line(5, 20, 4, 16, "moss")
    # a lily pad
    c.hline(15, 19, 23, "moss"); c.px(15, 23, "moss_lt")
    c.outline_silhouette()
    return c


def tent():
    c = Canvas()
    # canvas A-frame
    c.tri(15, 7, 27, 13, "parch")
    for i, y in enumerate(range(7, 28)):  # right face shaded
        w = round(13 * i / 20)
        c.hline(15 + max(1, w - 3), 15 + w, y, "parch_sh")
    c.hline(3, 28, 27, "parch_dk")
    # entrance flap
    c.tri(15, 15, 27, 4, "coal")
    c.vline(15, 15, 27, "coal_lt")
    # ridgepole + pennant
    c.vline(15, 3, 6, "wood")
    c.rect(16, 3, 21, 4, "red"); c.px(21, 3, "red_lt"); c.hline(16, 19, 5, "red_dk")
    c.outline_silhouette()
    return c


def grain():
    c = Canvas()
    # stalk
    c.vline(16, 14, 29, "straw"); c.vline(17, 14, 29, "gold_sh")
    # ear: tight chevron kernels hugging the stalk, tapering to a tip
    for i in range(7):
        y = 16 - i * 2
        w = 2 if i < 5 else 1
        c.rect(16 - w, y, 16, y + 1, "gold")
        c.rect(17, y, 17 + w, y + 1, "gold")
        c.px(16 - w, y, "gold_lt"); c.px(17 + w, y, "gold_sh")
    c.rect(16, 2, 17, 3, "gold"); c.px(16, 2, "gold_lt")  # tip
    # awns: fine whiskers off the top
    c.line(14, 3, 12, 0, "straw_lt"); c.line(19, 3, 21, 0, "straw_lt")
    c.px(16, 0, "straw_lt"); c.px(17, 1, "straw_lt")
    # two leaves low on the stalk
    c.line(15, 24, 9, 20, "moss"); c.line(15, 25, 10, 21, "moss_dk"); c.px(8, 19, "moss_lt")
    c.line(18, 26, 24, 23, "moss"); c.px(25, 22, "moss_lt")
    c.outline_silhouette()
    return c


def coal():
    c = Canvas()
    # low wide pile: flat base, three angular bumps on top
    c.rect(4, 20, 27, 27, "coal")
    c.tri(9, 13, 20, 6, "coal")
    c.tri(17, 10, 20, 6, "coal")
    c.tri(24, 15, 20, 5, "coal_lt")
    # seams between bumps
    c.line(13, 15, 12, 20, "outline"); c.line(21, 14, 21, 20, "outline")
    # facets and glints
    c.line(7, 17, 10, 14, "coal_lt"); c.line(15, 13, 17, 16, "coal_lt")
    c.hline(6, 12, 24, "coal_lt"); c.hline(16, 22, 25, "coal_lt")
    c.px(9, 15, "steel_lt"); c.px(18, 12, "snow_sh"); c.px(24, 17, "steel_lt")
    c.hline(5, 26, 27, "outline")
    c.outline_silhouette()
    return c


def infantry():
    c = Canvas()
    # musket B first (under): upper-left bayonet to lower-right stock
    thick_diag(c, 2, 4, 5, 7, "steel_lt")                      # bayonet
    thick_diag(c, 6, 8, 13, 15, "steel")                       # barrel
    thick_diag(c, 14, 16, 25, 27, "wood")                      # stock
    c.px(26, 28, "wood"); c.px(27, 28, "wood"); c.px(26, 27, "wood")  # butt flare
    # musket A on top: lower-left stock to upper-right bayonet
    thick_diag(c, 5, 27, 16, 16, "wood")
    c.px(4, 28, "wood"); c.px(5, 28, "wood"); c.px(4, 27, "wood")
    thick_diag(c, 17, 15, 24, 8, "steel")
    thick_diag(c, 25, 7, 28, 4, "steel_lt")
    # wood grain + barrel glint
    c.px(9, 24, "wood_lt"); c.px(12, 21, "wood_lt"); c.px(20, 22, "wood_sh")
    c.px(20, 12, "steel_lt"); c.px(10, 12, "steel_dk")
    # brass trigger guards near each stock's grip
    c.rect(14, 19, 15, 19, "gold"); c.rect(16, 20, 17, 20, "gold")
    c.outline_silhouette()
    return c


def frigate():
    c = Canvas()
    # hull
    c.rect(4, 22, 27, 25, "navy_dk")
    c.hline(5, 26, 26, "navy_dk")
    c.hline(4, 27, 22, "gold_sh")  # sheer stripe
    for x in (7, 12, 17, 22):      # gunports
        c.px(x, 24, "coal")
    c.line(27, 22, 29, 20, "navy_dk")  # bow rise
    # masts
    for x in (9, 16, 23):
        c.vline(x, 5, 21, "wood_sh")
    # sails
    c.rect(6, 7, 12, 12, "parch");  c.rect(6, 14, 12, 18, "parch_sh")
    c.rect(13, 6, 19, 12, "parch"); c.rect(13, 14, 19, 18, "parch_sh")
    c.rect(20, 8, 26, 12, "parch"); c.rect(20, 14, 26, 18, "parch_sh")
    for x0, x1 in ((6, 12), (13, 19), (20, 26)):  # billow shading
        c.vline(x0, 7, 18, "parch_dk")
    # pennant on the main (center) mast
    c.vline(16, 3, 5, "wood_sh")
    c.rect(17, 3, 20, 3, "red"); c.px(17, 4, "red_dk")
    # bowsprit off the bow
    c.line(28, 21, 31, 18, "wood_sh"); c.px(28, 22, "wood_sh")
    c.outline_silhouette()
    return c


def ironclad():
    c = Canvas()
    # low hull
    c.rect(3, 20, 28, 24, "coal")
    c.hline(4, 27, 25, "coal_lt")
    c.hline(3, 28, 20, "steel_dk")
    # armored casemate (sloped)
    for i, y in enumerate(range(13, 20)):
        c.hline(10 + (6 - i), 24 - (6 - i) // 2, y, "steel")
    c.rect(11, 14, 13, 19, "steel_lt")  # lit slope
    c.px(15, 16, "coal"); c.px(19, 16, "coal")  # ports
    # long thin gun barrel out the bow face
    c.rect(3, 16, 10, 16, "steel_dk"); c.px(3, 15, "steel_dk")
    # funnel centered on casemate top, puffs of smoke drifting aft
    c.rect(16, 6, 18, 12, "coal"); c.hline(16, 18, 6, "coal_lt")
    c.rect(20, 3, 21, 4, "steel_lt"); c.rect(23, 2, 24, 2, "steel_lt")
    # waterline
    c.hline(1, 30, 26, "navy")
    c.hline(3, 9, 27, "navy_lt"); c.hline(14, 20, 27, "navy_lt"); c.hline(24, 29, 27, "navy_lt")
    c.outline_silhouette()
    return c


def fort():
    c = Canvas()
    # tower body
    c.rect(7, 9, 24, 28, "grey_lt")
    c.rect(7, 9, 10, 28, "snow_sh")        # lit edge
    c.rect(21, 9, 24, 28, "grey")          # shaded edge
    # crenellations
    for x in (7, 12, 17, 22):
        c.rect(x, 5, x + 2, 8, "grey_lt")
    c.hline(7, 24, 9, "grey_dk")
    # stone courses (sparse, quiet)
    for y in (14, 19, 24):
        c.hline(8, 23, y, "grey")
    c.px(12, 12, "grey"); c.px(19, 16, "grey"); c.px(10, 21, "grey"); c.px(21, 26, "grey")
    # gate
    c.rect(13, 21, 18, 28, "wood_sh")
    c.hline(13, 18, 21, "wood"); c.px(13, 20, "wood"); c.px(18, 20, "wood")
    c.vline(15, 21, 28, "outline")  # door split
    # arrow slit + flag
    c.rect(15, 12, 16, 15, "coal")
    c.vline(25, 2, 8, "wood_sh"); c.rect(26, 2, 30, 4, "red"); c.hline(26, 29, 5, "red_dk")
    c.outline_silhouette()
    return c


def handshake():
    c = Canvas()
    # chunky horizontal sleeves: navy left, red right, slight vertical offset
    c.rect(1, 10, 8, 18, "navy"); c.hline(1, 8, 10, "navy_lt"); c.hline(1, 8, 18, "navy_dk")
    c.rect(24, 13, 30, 21, "red"); c.hline(24, 30, 13, "red_lt"); c.hline(24, 30, 21, "red_dk")
    # cuffs
    c.rect(9, 9, 11, 19, "parch"); c.vline(9, 9, 19, "parch_sh")
    c.rect(21, 12, 23, 22, "parch"); c.vline(23, 12, 22, "parch_sh")
    # clasped hands: big central mass
    c.rect(12, 10, 20, 21, "skin")
    # left hand's fingers wrap over the back of the right hand (bottom-right)
    for y in (16, 18, 20):
        c.hline(15, 20, y, "skin_sh")
    c.px(20, 15, "skin_sh"); c.px(20, 17, "skin_sh"); c.px(20, 19, "skin_sh")
    # right hand's thumb crossing over the top-left
    c.rect(12, 10, 14, 13, "skin_sh"); c.px(12, 10, "skin")
    c.hline(12, 15, 14, "skin_sh")  # crease between the two hands
    # knuckle bumps along the top of the left hand
    c.px(15, 9, "skin"); c.px(17, 9, "skin"); c.px(19, 9, "skin")
    c.outline_silhouette()
    return c


def _rancher_body(c):
    def hat(c):
        c.rect(10, 3, 16, 5, "sand"); c.px(13, 3, "sand_sh")  # dented crown
        c.rect(6, 6, 20, 7, "sand"); c.hline(7, 19, 6, "sand_sh")
        c.hline(10, 16, 5, "wood_sh")  # band
    _worker_base(c, hat, shirt="denim", arm="denim_lt", bib=None, legs="wood")
    # red bandana at the neck
    c.rect(11, 14, 15, 15, "red"); c.px(13, 16, "red_dk")


def rancher():
    c = Canvas()
    _rancher_body(c)
    # coiled lasso hanging from the right hand
    for dx, dy in ((0, -3), (0, 3), (-3, 0), (3, 0), (-2, -2), (2, -2), (-2, 2), (2, 2)):
        c.px(24 + dx, 23 + dy, "straw")
    c.px(24 + 3, 23 - 1, "straw_lt"); c.px(24 - 3, 23 + 1, "straw_lt")
    c.line(21, 22, 22, 21, "straw")  # rope to hand
    c.outline_silhouette()
    return c


def rancher_work():
    """Work frame: lasso loop spinning overhead."""
    c = Canvas()
    _rancher_body(c)
    # spinning loop above and right of the hat
    for dx, dy in ((0, -4), (0, 4), (-4, 0), (4, 0), (-3, -3), (3, -3), (-3, 3), (3, 3),
                   (-4, -1), (4, 1), (-1, -4), (1, 4)):
        c.px(24 + dx, 5 + dy, "straw")
    c.px(27, 3, "straw_lt"); c.px(21, 8, "straw_lt")
    # rope down to a raised hand
    c.line(23, 10, 21, 14, "straw")
    c.rect(19, 14, 21, 16, "skin")
    c.outline_silhouette()
    return c


def _forester_body(c):
    def hat(c):
        c.rect(9, 5, 17, 7, "pine"); c.rect(10, 4, 16, 4, "pine")
        c.hline(9, 12, 7, "pine_lt")
        c.hline(17, 20, 7, "pine_dk")  # small peak
    _worker_base(c, hat, shirt="wood", arm="wood_lt", bib="pine", bib_lt="pine_lt")


def forester():
    c = Canvas()
    _forester_body(c)
    # axe shouldered on the right: handle diagonal, steel head up top
    thick_diag(c, 20, 22, 26, 8, "wood_lt")
    c.rect(24, 4, 28, 7, "steel")
    c.vline(28, 4, 8, "steel_lt"); c.px(29, 5, "steel_lt"); c.px(29, 6, "steel_lt")
    c.px(24, 4, "steel_dk"); c.px(24, 7, "steel_dk")
    c.rect(19, 20, 22, 22, "skin")
    c.outline_silhouette()
    return c


def forester_work():
    """Work frame: axe swung down into the log, chips flying."""
    c = Canvas()
    _forester_body(c)
    thick_diag(c, 19, 13, 26, 21, "wood_lt")
    # axe head biting low, edge down-right
    c.rect(26, 22, 30, 25, "steel")
    c.hline(27, 31, 26, "steel_lt")
    c.px(26, 22, "steel_dk"); c.px(26, 25, "steel_dk")
    c.px(31, 28, "wood_lt"); c.px(29, 29, "wood_lt"); c.px(24, 28, "wood")  # chips
    c.rect(19, 14, 21, 16, "skin")
    c.outline_silhouette()
    return c


def _driller_body(c):
    def hat(c):
        c.rect(9, 5, 17, 7, "orange"); c.rect(11, 3, 15, 4, "orange")
        c.hline(11, 15, 3, "orange_lt"); c.hline(8, 18, 7, "orange_lt")
    _worker_base(c, hat, shirt="denim", arm="denim_lt", bib="coal", bib_lt="coal_lt",
                 legs="coal")


def _derrick(c):
    # mini oil derrick at his right: tapering truss tower
    c.line(22, 28, 25, 6, "wood_sh"); c.line(30, 28, 27, 6, "wood_sh")
    c.hline(25, 27, 5, "wood_sh")
    for y in (11, 16, 21, 26):  # crossbars
        w = (y - 5) // 4
        c.hline(26 - w, 26 + w, y, "wood")


def driller():
    c = Canvas()
    _driller_body(c)
    _derrick(c)
    c.px(26, 4, "gold")  # crown light
    c.outline_silhouette()
    return c


def driller_work():
    """Work frame: the derrick strikes oil — gusher over the crown."""
    c = Canvas()
    _driller_body(c)
    _derrick(c)
    # gusher spurting above the crown
    c.vline(26, 1, 4, "coal"); c.px(26, 0, "coal_lt")
    c.px(24, 2, "coal_lt"); c.px(28, 1, "coal_lt"); c.px(29, 3, "coal")
    # arm raised toward the rig
    c.line(20, 16, 22, 14, "denim_lt"); c.px(22, 13, "skin")
    c.outline_silhouette()
    return c


def _prospector_body(c):
    def hat(c):
        c.rect(10, 3, 16, 5, "wood_sh")
        c.rect(6, 6, 20, 7, "wood_sh"); c.hline(7, 19, 6, "wood")
    _worker_base(c, hat, shirt="parch_sh", arm="parch", bib="wood_sh", bib_lt="wood",
                 legs="grey_dk")


def prospector():
    c = Canvas()
    _prospector_body(c)
    # gold pan held out at the right, nuggets inside
    c.rect(21, 18, 29, 19, "steel_dk")
    c.rect(22, 16, 28, 17, "steel")
    c.hline(23, 27, 16, "steel_lt")
    c.px(24, 16, "gold"); c.px(26, 16, "gold_lt"); c.px(25, 15, "gold")
    c.line(20, 21, 22, 19, "skin")  # arm up to pan
    c.outline_silhouette()
    return c


def prospector_work():
    """Work frame: pan tilted, water sloshing out, gold staying in."""
    c = Canvas()
    _prospector_body(c)
    # tilted pan (high on the left, pouring right)
    c.line(21, 16, 29, 20, "steel_dk"); c.line(21, 17, 29, 21, "steel_dk")
    c.line(22, 15, 28, 18, "steel")
    c.px(22, 14, "steel_lt"); c.px(23, 15, "steel_lt")
    # gold nuggets held at the high side, water spilling off the low side
    c.px(23, 14, "gold"); c.px(24, 14, "gold_lt")
    c.px(30, 22, "denim_lt"); c.px(31, 24, "denim_lt"); c.px(29, 25, "denim_lt")
    c.line(20, 20, 21, 18, "skin")
    c.outline_silhouette()
    return c


def hills():
    c = Canvas()
    # two low, wide mounds with a clear seam between them
    # back-right mound (shaded)
    for i, y in enumerate(range(14, 28)):
        w = round(9 * (i + 3) / 14)
        c.hline(22 - w, min(22 + w, 31), y, "moss_dk")
    c.hline(19, 25, 15, "moss")
    # front-left mound (lit)
    for i, y in enumerate(range(16, 28)):
        w = round(10 * (i + 3) / 12)
        c.hline(max(10 - w, 0), 10 + w, y, "moss")
    c.hline(6, 13, 17, "moss_lt"); c.hline(4, 11, 19, "moss_lt")
    # seam
    c.line(17, 20, 16, 27, "outline")
    c.hline(1, 30, 27, "moss_dk")
    c.outline_silhouette()
    return c


def desert():
    c = Canvas()
    # sun
    c.disc(25, 6, 3, "gold_lt"); c.px(25, 6, "gold")
    # dunes
    c.disc(10, 27, 9, "sand"); c.rect(1, 26, 19, 29, "sand")
    c.disc(23, 28, 8, "sand_sh"); c.rect(15, 27, 30, 29, "sand_sh")
    c.hline(4, 14, 21, "sand_sh")  # dune crest shadow
    # cactus with two arms
    c.rect(6, 12, 7, 24, "moss")
    c.rect(3, 14, 4, 17, "moss"); c.hline(4, 5, 17, "moss")
    c.rect(9, 11, 10, 14, "moss"); c.hline(8, 9, 14, "moss")
    c.vline(6, 12, 24, "moss_lt")
    c.outline_silhouette()
    return c


def tundra():
    c = Canvas()
    # snow drift
    c.disc(12, 27, 8, "snow"); c.disc(22, 28, 7, "snow_sh")
    c.rect(3, 26, 29, 29, "snow")
    c.hline(16, 28, 27, "snow_sh")
    # bare twiggy shrub
    c.vline(20, 14, 25, "wood_sh")
    c.line(20, 18, 16, 13, "wood_sh"); c.line(20, 16, 24, 11, "wood_sh")
    c.px(15, 12, "wood"); c.px(25, 10, "wood"); c.px(20, 13, "wood")
    c.outline_silhouette()
    return c


def grassland():
    c = Canvas()
    # three fuller grass tufts, five blades each
    for cx, base, h in ((8, 27, 9), (19, 29, 11), (27, 26, 8)):
        c.vline(cx, base - h, base, "moss")
        c.vline(cx - 1, base - h + 2, base, "moss")
        c.vline(cx + 1, base - h + 2, base, "moss")
        c.line(cx - 2, base, cx - 4, base - h + 4, "moss")
        c.line(cx + 2, base, cx + 4, base - h + 4, "moss")
        c.px(cx, base - h, "moss_lt"); c.px(cx - 1, base - h + 2, "moss_lt")
        c.px(cx - 4, base - h + 4, "moss_lt"); c.px(cx + 4, base - h + 4, "moss_lt")
        c.hline(cx - 4, cx + 4, base, "moss_dk")
    c.outline_silhouette()
    return c


SAMPLES = [
    ("civilians/Farmer", farmer),
    ("civilians/Miner", miner),
    ("civilians/Engineer", engineer),
    ("terrain/Mountain", mountain),
    ("terrain/Forest", forest),
    ("terrain/Swamp", swamp),
    ("ui/Tent", tent),
    ("commodities/Grain", grain),
    ("commodities/Coal", coal),
    ("units/Infantry", infantry),
    ("ships/Frigate", frigate),
    ("ships/Ironclad", ironclad),
    ("infrastructure/Fort", fort),
    ("diplomacy/NonAggressionPact", handshake),
]

BATCH1 = [
    ("civilians/Farmer", farmer),
    ("civilians/Miner", miner),
    ("civilians/Engineer", engineer),
    ("civilians/Rancher", rancher),
    ("civilians/Forester", forester),
    ("civilians/Driller", driller),
    ("civilians/Prospector", prospector),
    ("terrain/Mountain", mountain),
    ("terrain/Hills", hills),
    ("terrain/Forest", forest),
    ("terrain/Swamp", swamp),
    ("terrain/Desert", desert),
    ("terrain/Tundra", tundra),
    ("terrain/Grassland", grassland),
    ("ui/Tent", tent),
]

# ── Mid-swing frames (between the rest pose and the action pose) ────────


def farmer_work_mid():
    """Mid frame: pitchfork lifted level, mid-pitch."""
    c = Canvas()
    _farmer_body(c)
    thick_diag(c, 19, 16, 28, 11, "wood")
    c.hline(25, 31, 9, "steel_dk")                  # crossbar
    c.vline(26, 4, 8, "steel"); c.vline(28, 4, 8, "steel"); c.vline(30, 5, 8, "steel")
    c.rect(19, 15, 21, 17, "skin")
    c.outline_silhouette()
    return c


def miner_work_mid():
    """Mid frame: pick swung level, head leading."""
    c = Canvas()
    _miner_body(c)
    thick_diag(c, 19, 17, 27, 13, "wood")
    # head vertical at the leading end, tips curling back
    c.vline(29, 9, 18, "steel"); c.vline(30, 10, 16, "steel_lt")
    c.px(28, 8, "steel_dk"); c.px(27, 7, "steel_dk")
    c.px(28, 19, "steel_dk"); c.px(27, 20, "steel_dk")
    c.rect(19, 15, 21, 17, "skin")
    c.outline_silhouette()
    return c


def engineer_work_mid():
    """Mid frame: wrench tilted, starting the swing."""
    c = Canvas()
    _engineer_body(c)
    thick_diag(c, 23, 19, 25, 9, "steel")
    c.px(23, 18, "steel_lt")
    c.rect(23, 4, 28, 5, "steel")
    c.rect(23, 8, 28, 9, "steel")
    c.rect(23, 5, 24, 8, "steel")
    c.px(23, 4, "steel_lt")
    c.rect(20, 18, 22, 20, "skin")
    c.outline_silhouette()
    return c


def rancher_work_mid():
    """Mid frame: small loop at shoulder height, winding up."""
    c = Canvas()
    _rancher_body(c)
    for dx, dy in ((0, -3), (0, 3), (-3, 0), (3, 0), (-2, -2), (2, -2), (-2, 2), (2, 2)):
        c.px(25 + dx, 12 + dy, "straw")
    c.px(27, 10, "straw_lt")
    c.line(23, 14, 21, 16, "straw")
    c.rect(19, 15, 21, 17, "skin")
    c.outline_silhouette()
    return c


def forester_work_mid():
    """Mid frame: axe raised overhead, wound up."""
    c = Canvas()
    _forester_body(c)
    thick_diag(c, 20, 17, 23, 6, "wood_lt")
    c.rect(22, 2, 26, 5, "steel")
    c.hline(25, 27, 6, "steel_lt"); c.px(27, 5, "steel_lt")
    c.px(22, 2, "steel_dk"); c.px(22, 5, "steel_dk")
    c.rect(19, 16, 21, 18, "skin")
    c.outline_silhouette()
    return c


def driller_work_mid():
    """Mid frame: the well starts to blow — small spurt at the crown."""
    c = Canvas()
    _driller_body(c)
    _derrick(c)
    c.px(26, 4, "gold")
    c.px(26, 3, "coal"); c.px(26, 2, "coal_lt")
    c.line(20, 17, 21, 15, "denim_lt"); c.px(21, 14, "skin")
    c.outline_silhouette()
    return c


def prospector_work_mid():
    """Mid frame: pan half-tilted, water rippling."""
    c = Canvas()
    _prospector_body(c)
    c.line(21, 17, 29, 19, "steel_dk"); c.line(21, 18, 29, 20, "steel_dk")
    c.line(22, 16, 28, 17, "steel")
    c.px(22, 15, "steel_lt")
    c.px(24, 15, "gold"); c.px(25, 15, "gold_lt")
    c.px(26, 16, "denim_lt"); c.px(30, 21, "denim_lt")
    c.line(20, 20, 21, 18, "skin")
    c.outline_silhouette()
    return c


# Frame 1 = mid-swing, frame 2 = full action; the map plays rest→1→2→1.
WORK_FRAMES = [
    ("civilians/FarmerWork1", farmer_work_mid),
    ("civilians/FarmerWork2", farmer_work),
    ("civilians/MinerWork1", miner_work_mid),
    ("civilians/MinerWork2", miner_work),
    ("civilians/EngineerWork1", engineer_work_mid),
    ("civilians/EngineerWork2", engineer_work),
    ("civilians/RancherWork1", rancher_work_mid),
    ("civilians/RancherWork2", rancher_work),
    ("civilians/ForesterWork1", forester_work_mid),
    ("civilians/ForesterWork2", forester_work),
    ("civilians/DrillerWork1", driller_work_mid),
    ("civilians/DrillerWork2", driller_work),
    ("civilians/ProspectorWork1", prospector_work_mid),
    ("civilians/ProspectorWork2", prospector_work),
]
