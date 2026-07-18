"""Regenerate every pixel-art icon SVG into assets-src/icons/.

Usage: python3 gen.py   (then `cargo run -p gen_assets` to refresh the PNGs)
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import ground
import pixelkit
import rail
import splash
import sprites
import sprites2
import sprites3
import sprites4

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")

ALL = dict(sprites.BATCH1)
for batch in (
    sprites.WORK_FRAMES,
    sprites2.BATCH2,
    sprites3.BATCH3,
    sprites4.BATCH4,
    ground.GROUND,
    rail.RAIL,
    splash.SPLASH,
):
    ALL.update(dict(batch))

for name, fn in sorted(ALL.items()):
    path = os.path.join(ROOT, name + ".svg")
    with open(path, "w") as f:
        f.write(pixelkit.to_svg(fn()))
    print("wrote", name)
print(f"{len(ALL)} SVGs regenerated")
