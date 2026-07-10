"""Pixel-art authoring kit: draw on a 32x32 palette-indexed canvas, emit
pixel-rect SVG (pipeline-compatible with gen_assets) and preview PNGs."""

from PIL import Image

SIZE = 32

# Shared 19th-century muted palette. Single source of truth for every sprite.
PAL = {
    "outline":  "#2a2418",
    "skin":     "#e0af80", "skin_sh": "#bd8a5c",
    "parch":    "#f2e6c8", "parch_sh": "#d9c49a", "parch_dk": "#bda87c",
    "wood":     "#8a5a2b", "wood_sh": "#6b4320", "wood_lt": "#a9743c",
    "gold":     "#e3b341", "gold_sh": "#b3822a", "gold_lt": "#f4d477",
    "navy":     "#2c4a66", "navy_lt": "#47698c", "navy_dk": "#1d3347",
    "steel":    "#9aa0a6", "steel_lt": "#c4c9cd", "steel_dk": "#6f767d",
    "coal":     "#3a3530", "coal_lt": "#575049",
    "moss":     "#5f8a3a", "moss_dk": "#46682b", "moss_lt": "#7ba652",
    "pine":     "#3a6b40", "pine_dk": "#2a4f30", "pine_lt": "#4f8752",
    "red":      "#a33b2e", "red_lt": "#c25545", "red_dk": "#7d2c22",
    "snow":     "#f4f4ee", "snow_sh": "#cfd4d6",
    "sky":      "#a8c4cc",
    "murk":     "#4a6a55", "murk_dk": "#38513f", "murk_lt": "#5d8168",
    "sand":     "#d9b872", "sand_sh": "#b7935a",
    "grey":     "#7a7068", "grey_lt": "#9c938a", "grey_dk": "#5b534c",
    "straw":    "#d9a441", "straw_lt": "#e8c46a",
    "orange":   "#cf7530", "orange_lt": "#e0904a",
    "denim":    "#5a7186", "denim_lt": "#748ba0",
    "teal":     "#3e8f8a", "teal_lt": "#62b5ae", "teal_dk": "#2b6b66",
}


class Canvas:
    def __init__(self, size=SIZE, height=None):
        self.size = size
        self.w = size
        self.h = height if height is not None else size
        self.g = [[None] * self.w for _ in range(self.h)]

    def px(self, x, y, c):
        if 0 <= x < self.w and 0 <= y < self.h:
            self.g[y][x] = c

    def hline(self, x0, x1, y, c):
        for x in range(min(x0, x1), max(x0, x1) + 1):
            self.px(x, y, c)

    def vline(self, x, y0, y1, c):
        for y in range(min(y0, y1), max(y0, y1) + 1):
            self.px(x, y, c)

    def rect(self, x0, y0, x1, y1, c):
        for y in range(y0, y1 + 1):
            self.hline(x0, x1, y, c)

    def frame(self, x0, y0, x1, y1, c):
        self.hline(x0, x1, y0, c); self.hline(x0, x1, y1, c)
        self.vline(x0, y0, y1, c); self.vline(x1, y0, y1, c)

    def disc(self, cx, cy, r, c):
        for y in range(cy - r, cy + r + 1):
            for x in range(cx - r, cx + r + 1):
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r * 0.6:
                    self.px(x, y, c)

    def line(self, x0, y0, x1, y1, c):
        dx, dy = abs(x1 - x0), -abs(y1 - y0)
        sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
        e = dx + dy
        while True:
            self.px(x0, y0, c)
            if x0 == x1 and y0 == y1:
                break
            e2 = 2 * e
            if e2 >= dy: e += dy; x0 += sx
            if e2 <= dx: e += dx; y0 += sy

    def tri(self, apex_x, apex_y, base_y, half_w, c):
        """Filled isoceles triangle, apex up."""
        h = base_y - apex_y
        for i, y in enumerate(range(apex_y, base_y + 1)):
            w = round(half_w * i / max(h, 1))
            self.hline(apex_x - w, apex_x + w, y, c)

    def outline_silhouette(self, c="outline"):
        """1px outline around every non-transparent region (drawn outside)."""
        add = []
        for y in range(self.h):
            for x in range(self.w):
                if self.g[y][x] is None:
                    for dx, dy in ((1,0),(-1,0),(0,1),(0,-1)):
                        nx, ny = x + dx, y + dy
                        if 0 <= nx < self.w and 0 <= ny < self.h \
                                and self.g[ny][nx] not in (None, c):
                            add.append((x, y)); break
        for x, y in add:
            self.g[y][x] = c


def to_svg(canvas):
    """Emit pixel-rect SVG, horizontal runs merged. viewBox matches grid."""
    w, h = canvas.w, canvas.h
    rows = []
    for y in range(h):
        x = 0
        while x < w:
            c = canvas.g[y][x]
            if c is None:
                x += 1; continue
            x0 = x
            while x < w and canvas.g[y][x] == c:
                x += 1
            rows.append(f'<rect x="{x0}" y="{y}" width="{x - x0}" height="1" fill="{PAL[c]}"/>')
    body = "\n".join(rows)
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" '
            f'shape-rendering="crispEdges">\n{body}\n</svg>\n')


def to_png(canvas, path, scale=2):
    w, h = canvas.w, canvas.h
    im = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    for y in range(h):
        for x in range(w):
            c = canvas.g[y][x]
            if c is not None:
                hx = PAL[c].lstrip("#")
                im.putpixel((x, y), tuple(int(hx[i:i+2], 16) for i in (0, 2, 4)) + (255,))
    im = im.resize((w * scale, h * scale), Image.NEAREST)
    im.save(path)
    return im


def sheet(canvases, labels, path, scale=4, cols=7, bg=(74, 86, 66, 255)):
    """Contact sheet for review, scaled with NEAREST."""
    from PIL import ImageDraw
    cell = SIZE * scale; pad = 10; lab = 16
    rows = (len(canvases) + cols - 1) // cols
    im = Image.new("RGBA", (cols * (cell + pad) + pad, rows * (cell + pad + lab) + pad), bg)
    d = ImageDraw.Draw(im)
    for i, (cv, name) in enumerate(zip(canvases, labels)):
        tile = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
        for y in range(SIZE):
            for x in range(SIZE):
                c = cv.g[y][x]
                if c is not None:
                    h = PAL[c].lstrip("#")
                    tile.putpixel((x, y), tuple(int(h[j:j+2], 16) for j in (0, 2, 4)) + (255,))
        tile = tile.resize((cell, cell), Image.NEAREST)
        x = pad + (i % cols) * (cell + pad); y = pad + (i // cols) * (cell + pad + lab)
        im.alpha_composite(tile, (x, y))
        d.text((x, y + cell + 2), name, fill=(240, 240, 230, 255))
    im.save(path)
