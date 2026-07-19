"""Terrain motif variants with the resource woven into the tile art.

Cards #542 / #540: the map no longer overlays commodity icons on hexes —
a tile with a (visible) resource swaps to a `terrain/<Terrain><Resource>`
motif that integrates the resource into the hex design. Every variant
composes on the plain terrain motif from `sprites.py` so the two versions
of a hex read as the same place, with and without the find.

Forests are special (#540): a Forest tile with Timber keeps the vivid
`terrain/Forest` pines (good, developable timber); a Forest tile without
Timber renders the desaturated, sparser `terrain/ForestScrub` instead.

The (terrain, resource) pairs here are exactly the ones map generation
produces (`roll_surface_resource` / `random_mineral_deposit` in
`crates/domain/src/map/generator.rs`).
"""

import pixelkit
import sprites
from pixelkit import Canvas

# Stream water colors (card #539). "river" matches the map renderer's river
# polyline color (rgba 68/140/220) so the sprite's outflow stream reads as
# the same water as the meandering course it feeds.
pixelkit.PAL.setdefault("river", "#448cdc")
pixelkit.PAL.setdefault("river_lt", "#7fb4e8")
pixelkit.PAL.setdefault("river_dk", "#2f66a4")


# ── Shared resource elements ────────────────────────────────────────────


def _sheep(c, x, y):
    """Tiny grazing sheep: fleece blob, dark head + legs. (x, y) = head end."""
    c.rect(x - 4, y - 2, x - 1, y, "snow")
    c.px(x - 4, y - 3, "snow")
    c.px(x - 2, y - 3, "snow")
    c.px(x - 3, y, "snow_sh")
    c.rect(x, y - 2, x, y - 1, "coal")  # head, grazing low
    c.px(x - 4, y + 1, "coal")
    c.px(x - 1, y + 1, "coal")  # legs


def _lumps(c, spots, main, lit):
    """2x2 mineral lumps with a glint pixel."""
    for x, y in spots:
        c.rect(x, y, x + 1, y + 1, main)
        c.px(x, y, lit)


def _crystals(c, x, base):
    """Cluster of three gem spikes growing from `base`."""
    for dx, h in ((-2, 3), (0, 5), (2, 4)):
        c.vline(x + dx, base - h, base, "teal")
        c.px(x + dx, base - h, "teal_lt")
        c.px(x + dx, base, "teal_dk")
    c.hline(x - 3, x + 3, base + 1, "teal_dk")


def _oil_seep(c, x0, x1, y):
    """Dark seep pool with an oily sheen and a rising drip."""
    c.rect(x0, y, x1, y + 2, "coal")
    c.hline(x0 - 1, x0, y + 1, "coal")
    c.hline(x1, x1 + 1, y + 1, "coal")
    c.hline(x0 + 2, x1 - 2, y, "coal_lt")  # sheen
    mid = (x0 + x1) // 2
    c.px(mid, y - 2, "coal")
    c.px(mid + 1, y - 4, "coal_lt")  # sputtering drops


# ── Grassland (Grain / Fruit / Cotton / Livestock / Horses) ─────────────


def grassland_grain():
    c = Canvas()
    # Two staggered rows of wheat: back row shorter, front row taller.
    for row, (base, h, xs) in enumerate((
        (23, 7, (5, 10, 15, 20, 25)),
        (29, 9, (3, 8, 13, 18, 23, 28)),
    )):
        for x in xs:
            x += row  # slight stagger
            c.vline(x, base - h + 3, base, "straw")
            c.rect(x - 1, base - h, x + 1, base - h + 2, "gold")
            c.px(x, base - h, "straw_lt")
            c.px(x - 1, base - h + 2, "gold_sh")
        c.hline(1, 30, base, "moss_dk")
    c.outline_silhouette()
    return c


def grassland_fruit():
    c = sprites.grassland()
    # Round-canopy orchard tree behind the tufts, studded with red fruit.
    c.rect(14, 19, 15, 26, "wood")
    c.vline(14, 19, 26, "wood_lt")
    c.disc(14, 12, 6, "moss")
    c.disc(12, 10, 3, "moss_lt")
    c.hline(11, 18, 17, "moss_dk")
    for x, y in ((10, 11), (15, 8), (18, 13), (12, 15), (17, 16)):
        c.px(x, y, "red")
        c.px(x + 1, y, "red_lt")
    c.outline_silhouette()
    return c


def grassland_cotton():
    c = Canvas()
    # Three low cotton bushes heavy with white bolls.
    for cx, base, r in ((7, 26, 3), (17, 28, 4), (26, 25, 3)):
        c.disc(cx, base - r, r, "moss_dk")
        c.disc(cx - 1, base - r - 1, r - 1, "moss")
        for dx, dy in ((-r + 1, -1), (0, -r), (r - 1, -1), (0, 0)):
            c.rect(cx + dx, base - r + dy - 1, cx + dx + 1, base - r + dy, "snow")
            c.px(cx + dx + 1, base - r + dy, "snow_sh")
        c.hline(cx - r, cx + r, base + 1, "moss_dk")
    c.outline_silhouette()
    return c


def grassland_livestock():
    c = sprites.grassland()
    # Side-view cow grazing among the tufts.
    c.rect(10, 16, 21, 22, "wood")
    c.disc(11, 18, 2, "wood")
    c.disc(20, 17, 2, "wood")
    c.rect(14, 17, 17, 20, "snow")  # hide patch
    c.px(19, 19, "snow")
    c.rect(21, 13, 24, 17, "wood")  # head, lowered to graze
    c.rect(23, 15, 24, 17, "snow")  # muzzle blaze
    c.px(22, 12, "steel_lt")
    c.px(24, 12, "steel_lt")  # horns
    c.px(21, 14, "outline")  # eye
    for x in (11, 14, 18, 21):
        c.vline(x, 22, 26, "wood_sh")
    c.line(10, 17, 8, 20, "wood_sh")  # tail
    c.outline_silhouette()
    return c


def grassland_horses():
    c = sprites.grassland()
    # Standing horse in profile.
    c.rect(11, 15, 21, 20, "wood_lt")
    c.disc(12, 17, 2, "wood_lt")
    c.rect(20, 9, 22, 16, "wood_lt")  # neck
    c.rect(21, 8, 25, 11, "wood_lt")  # head
    c.px(25, 11, "wood_sh")  # muzzle
    c.px(22, 9, "outline")  # eye
    c.vline(20, 8, 15, "wood_sh")  # mane
    c.px(21, 7, "wood_sh")
    for x in (12, 14, 18, 20):
        c.vline(x, 20, 26, "wood_sh")
    c.line(11, 16, 8, 21, "wood_sh")  # tail
    c.px(8, 22, "wood_sh")
    c.outline_silhouette()
    return c


# ── Forest (#540: scrub variant for timberless forest) ──────────────────


def forest_scrub():
    """Sparse, washed-out stand: at most one timber's worth, undevelopable."""
    c = Canvas()
    # Two thin, greyed-green trees (murk palette, not the vivid pines).
    for i, (x, tiers, trunk_top) in enumerate((
        (9, ((8, 13, 3), (12, 19, 5)), 20),
        (21, ((13, 17, 3), (16, 22, 4)), 23),
    )):
        shade = "murk" if i == 0 else "murk_dk"
        for apex, base, hw in tiers:
            c.tri(x, apex, base, hw, shade)
        for apex, base, hw in tiers:
            for j, y in enumerate(range(apex, base + 1)):
                w = round(hw * j / (base - apex))
                c.px(x - w, y, "murk_lt")
        c.rect(x - 1, trunk_top, x, 27, "wood_sh")
    # A bare snag between them: this wood is thinning out.
    c.vline(27, 16, 27, "wood_sh")
    c.line(27, 19, 30, 16, "wood_sh")
    c.line(27, 22, 25, 19, "wood_sh")
    # Sparse ground scrub.
    c.hline(4, 7, 29, "murk_dk")
    c.hline(16, 20, 29, "murk_dk")
    c.px(25, 29, "murk_dk")
    c.outline_silhouette()
    return c


# ── Hills (Wool / Coal / Iron / Gold / Gems) ────────────────────────────


def hills_wool():
    c = sprites.hills()
    _sheep(c, 10, 21)
    _sheep(c, 26, 19)
    _sheep(c, 17, 25)
    c.outline_silhouette()
    return c


def hills_coal():
    c = sprites.hills()
    # Exposed seam wedge on the lit mound plus spoil lumps at the foot.
    for i, y in enumerate(range(21, 25)):
        c.hline(6 - i, 12 - i, y, "coal")
    c.px(8, 22, "coal_lt")
    c.px(10, 23, "coal_lt")
    _lumps(c, ((14, 25), (18, 26)), "coal", "coal_lt")
    c.outline_silhouette()
    return c


def hills_iron():
    c = sprites.hills()
    # Rust-flecked ore boulders breaking the turf.
    c.disc(9, 23, 2, "grey")
    c.disc(15, 25, 2, "grey_dk")
    c.disc(24, 22, 2, "grey")
    c.px(8, 22, "grey_lt")
    c.px(23, 21, "grey_lt")
    for x, y in ((9, 24), (16, 25), (25, 23), (10, 22)):
        c.px(x, y, "orange")
    c.outline_silhouette()
    return c


def hills_gold():
    c = sprites.hills()
    _lumps(c, ((8, 23), (12, 21), (16, 25), (24, 22)), "gold", "gold_lt")
    c.px(10, 25, "gold_sh")
    c.px(25, 24, "gold_sh")
    c.px(13, 20, "gold_lt")  # glint
    c.outline_silhouette()
    return c


def hills_gems():
    c = sprites.hills()
    _crystals(c, 10, 24)
    _crystals(c, 24, 22)
    c.outline_silhouette()
    return c


# ── Mountain (Coal / Iron / Gold / Gems) ────────────────────────────────


def mountain_coal():
    c = sprites.mountain()
    # Seam wedge low on the scree apron, spoil lumps below.
    c.hline(14, 23, 24, "coal")
    c.hline(13, 25, 25, "coal")
    c.hline(15, 24, 26, "coal")
    c.px(17, 25, "coal_lt")
    c.px(21, 24, "coal_lt")
    _lumps(c, ((26, 26), (9, 26)), "coal", "coal_lt")
    c.outline_silhouette()
    return c


def mountain_iron():
    c = sprites.mountain()
    # Ore boulders shed onto the lower slope, rust-stained.
    c.disc(8, 26, 2, "grey_dk")
    c.disc(27, 25, 2, "grey")
    c.disc(17, 27, 2, "grey_dk")
    c.px(26, 24, "grey_lt")
    for x, y in ((8, 25), (27, 26), (17, 26), (19, 28)):
        c.px(x, y, "orange")
    c.outline_silhouette()
    return c


def mountain_gold():
    c = sprites.mountain()
    # A vein zigzagging down the lit face, nuggets at the foot.
    c.line(9, 12, 12, 17, "gold")
    c.line(12, 17, 10, 22, "gold")
    c.line(10, 22, 13, 26, "gold")
    c.px(10, 13, "gold_lt")
    c.px(11, 23, "gold_lt")
    _lumps(c, ((24, 26), (28, 27)), "gold", "gold_lt")
    c.outline_silhouette()
    return c


def mountain_gems():
    c = sprites.mountain()
    _crystals(c, 26, 26)
    c.px(6, 27, "teal")
    c.px(7, 26, "teal_lt")
    c.outline_silhouette()
    return c


# ── Mountain river sources (card #539) ──────────────────────────────────
#
# Mountains where a river originates show a meltwater stream flowing out of
# the flank. Three appearance variants (left cascade / right-groove cascade /
# front falls) are COMPOSED over every mountain base — plain and each
# resource variant — at generation time, so 5 bases x 3 streams = 15 sprites
# without hand-drawing any combination. The map renderer picks a variant by
# hashing the hex coords (`river_source_variant` in
# `crates/presentation/src/map/layers.rs`).


def _stream_path(c, path):
    """2px-wide watercourse along `path` (head first), with sheen + foam."""
    for i, (x, y) in enumerate(path):
        c.px(x, y, "river")
        c.px(x + 1, y, "river_dk" if i % 3 == 2 else "river")
        if i % 4 == 1:
            c.px(x, y, "river_lt")
    x0, y0 = path[0]
    c.px(x0, y0 - 1, "snow")  # spring foam at the head
    c.px(x0 + 1, y0, "snow_sh")


def _pool(c, x0, x1, y):
    """Small pool where the stream leaves the rock and meets the ground."""
    c.rect(x0, y, x1, y + 1, "river")
    c.hline(x0 + 1, x1 - 1, y, "river_lt")
    c.px(x0, y + 1, "river_dk")
    c.px(x1, y + 1, "river_dk")


def _stream_left(c):
    """V1: cascade meandering down the lit left face, pooling bottom-left."""
    _stream_path(c, [
        (8, 11), (8, 12), (7, 13), (7, 14), (6, 15), (7, 16), (6, 17),
        (5, 18), (6, 19), (5, 20), (4, 21), (4, 22), (3, 23), (4, 24),
        (3, 25), (2, 26), (2, 27), (1, 28),
    ])
    _pool(c, 1, 5, 29)


def _stream_right(c):
    """V2: stream down the groove between the peaks, out bottom-right."""
    _stream_path(c, [
        (18, 16), (19, 17), (18, 18), (19, 19), (20, 20), (19, 21),
        (20, 22), (21, 23), (20, 24), (21, 25), (22, 26), (21, 27),
        (22, 28),
    ])
    _pool(c, 21, 26, 29)


def _stream_front(c):
    """V3: waterfall down the front face below the snowcap jag."""
    _stream_path(c, [
        (13, 11), (14, 12), (13, 13), (14, 14), (14, 15), (15, 16),
        (14, 17), (15, 18), (15, 19), (16, 20), (15, 21), (16, 22),
        (16, 23), (15, 24), (16, 25), (15, 26), (16, 27), (15, 28),
    ])
    _pool(c, 13, 18, 29)


_STREAMS = (_stream_left, _stream_right, _stream_front)


def _with_stream(base_fn, stream):
    """Compose stream variant `stream` (1-based) over a mountain base."""

    def build():
        c = base_fn()
        _STREAMS[stream - 1](c)
        c.outline_silhouette()
        return c

    return build


# ── Oil terrains (Desert / Swamp / Tundra) ──────────────────────────────


def desert_oil():
    c = sprites.desert()
    # Black seep pooling in the dune hollow, clear of the cactus.
    _oil_seep(c, 18, 27, 25)
    c.outline_silhouette()
    return c


def swamp_oil():
    c = sprites.swamp()
    # Oil slick riding the pool surface.
    c.hline(8, 15, 23, "coal")
    c.hline(10, 13, 22, "coal")
    c.hline(19, 26, 25, "coal")
    c.px(11, 22, "coal_lt")
    c.px(21, 25, "coal_lt")
    c.px(24, 24, "coal_lt")
    c.outline_silhouette()
    return c


def tundra_oil():
    c = sprites.tundra()
    # Dark seep staining the snowfield.
    _oil_seep(c, 5, 12, 25)
    c.outline_silhouette()
    return c


# `terrain/<Terrain><Resource>` names mirror what the map renderer looks
# up for a tile whose resource is visible (`terrain_motif_name` in
# `crates/presentation/src/map/layers.rs`).
VARIANTS = [
    ("terrain/GrasslandGrain", grassland_grain),
    ("terrain/GrasslandFruit", grassland_fruit),
    ("terrain/GrasslandCotton", grassland_cotton),
    ("terrain/GrasslandLivestock", grassland_livestock),
    ("terrain/GrasslandHorses", grassland_horses),
    ("terrain/ForestScrub", forest_scrub),
    ("terrain/HillsWool", hills_wool),
    ("terrain/HillsCoal", hills_coal),
    ("terrain/HillsIron", hills_iron),
    ("terrain/HillsGold", hills_gold),
    ("terrain/HillsGems", hills_gems),
    ("terrain/MountainCoal", mountain_coal),
    ("terrain/MountainIron", mountain_iron),
    ("terrain/MountainGold", mountain_gold),
    ("terrain/MountainGems", mountain_gems),
    ("terrain/DesertOil", desert_oil),
    ("terrain/SwampOil", swamp_oil),
    ("terrain/TundraOil", tundra_oil),
]

# River-source mountains (card #539): every mountain base x every stream
# variant, named `Mountain[<Resource>]River<1..3>` to match
# `terrain_motif_name`'s suffix composition.
_MOUNTAIN_BASES = [
    ("Mountain", sprites.mountain),
    ("MountainCoal", mountain_coal),
    ("MountainIron", mountain_iron),
    ("MountainGold", mountain_gold),
    ("MountainGems", mountain_gems),
]
VARIANTS += [
    (f"terrain/{name}River{v}", _with_stream(fn, v))
    for name, fn in _MOUNTAIN_BASES
    for v in (1, 2, 3)
]
