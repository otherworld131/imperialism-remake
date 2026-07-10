# Bundled fonts

The game bundles these OFL-licensed fonts under
`crates/presentation/assets/fonts/` (loaded in `theme.rs`):

| File | Family | Role |
|------|--------|------|
| `Jersey15-Regular.ttf` | Jersey 15 (Google Fonts, OFL) | Pixel-art UI font, used everywhere (regular/semibold/italic slots all map to it — hierarchy comes from size + color) |
| `UnifrakturCook-Bold.ttf` | UnifrakturCook (OFL) | Newspaper masthead flavor |

## Jersey 15 glyph patch

Upstream Jersey 15 lacks the geometric UI glyphs the widgets use
(`▲ ▼` table-sort / dropdown arrows, `✕` modal close, `✓`). The bundled
TTF is therefore **not** the upstream file: `patch_glyphs.py` draws those
four glyphs as pixel blocks matched to the font's grid and writes the
patched TTF.

To update the font, download the upstream file and re-run:

```bash
python3 patch_glyphs.py /path/to/upstream/Jersey15-Regular.ttf \
    ../../crates/presentation/assets/fonts/Jersey15-Regular.ttf
```

Requires `fonttools` (`pip install fonttools`).

## Why Jersey 15?

Chosen from the OFL pixel-font candidates (Pixelify Sans, VT323,
Silkscreen, DotGothic16) because it is the only one whose digits stay
unambiguous at ledger/tooltip sizes — Pixelify Sans renders `5` like `S`
and `C` like `O`, which is disqualifying for a numbers-heavy strategy
game. Pixel fonts ship no italic or bold cuts.
