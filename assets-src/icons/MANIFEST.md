# Icon Manifest

Source of truth for the icon registry. Every icon is authored as a handcrafted
64x64 SVG under `assets-src/icons/<group>/<Name>.svg` and rasterized by
`cargo run -p gen_assets` to `crates/presentation/assets/icons/<group>/<Name>.png`
(64x64 PNG, transparent background).

Names mirror `web/src/resourceEmoji.ts` keys and domain type names exactly
(`ResourceType` / `MaterialType` / `GoodsType` in `crates/domain/src/types.rs`,
unit/ship `type` and `category` names from the `get_buildable_units` contract).
Note: the material is `CannedFood` (enum name); the display alias `"Canned Food"`
maps to the same icon.

Shared style: **pixel art** on a 32×32 grid, scaled exactly 2× to the 64×64
output. Every icon is authored in `pixel-src/` (Python: one function per
sprite drawing on a palette-indexed canvas) and emitted as a pixel-rect SVG,
so the SVG stays the pipeline's source of truth. All sprites share the muted
19th-century palette in `pixel-src/pixelkit.py` (single source of truth) and
a 1px silhouette outline `#2a2418`. The game loads these PNGs with
nearest-neighbor sampling (`map/icons.rs`) so the pixels stay crisp at any
map zoom.

To edit an icon: change its function in `pixel-src/sprites*.py`, then run
`python3 assets-src/icons/pixel-src/gen.py && cargo run -p gen_assets`.
Hand-drawn replacement PNGs/SVGs also work — the pipeline doesn't care how
the SVG was made.

## Replacing an icon

Icons are loose files loaded from disk at startup — **no recompile needed**:

1. **Quick swap**: overwrite `crates/presentation/assets/icons/<group>/<Name>.png`
   with any 64x64 transparent PNG and restart the game.
2. **Source-of-truth swap**: replace `assets-src/icons/<group>/<Name>.svg` and run
   `cargo run -p gen_assets` (rasterizes every SVG; idempotent).
3. **New icon**: add the SVG, run the tool — the loader auto-discovers any
   `icons/<group>/<Name>.png`; code looks icons up by `(group, Name)` only.

Keep the canonical names from this manifest; they match the domain type names
the view models emit.

## commodities/ (22)

| Name | Output | Pictogram |
|------|--------|-----------|
| Grain | `crates/presentation/assets/icons/commodities/Grain.png` | Golden wheat ear on stalk with leaf |
| Fruit | `crates/presentation/assets/icons/commodities/Fruit.png` | Red apple with stem and leaf |
| Cotton | `crates/presentation/assets/icons/commodities/Cotton.png` | White boll cluster on branched stem |
| Wool | `crates/presentation/assets/icons/commodities/Wool.png` | Cream fleece cloud with curl marks |
| Timber | `crates/presentation/assets/icons/commodities/Timber.png` | Standing three-tier pine tree |
| Livestock | `crates/presentation/assets/icons/commodities/Livestock.png` | Front-view cow head with horns |
| Fish | `crates/presentation/assets/icons/commodities/Fish.png` | Side-view blue fish with fins |
| Horses | `crates/presentation/assets/icons/commodities/Horses.png` | Knight-style horse head with mane |
| Coal | `crates/presentation/assets/icons/commodities/Coal.png` | Heap of black faceted lumps |
| Iron | `crates/presentation/assets/icons/commodities/Iron.png` | Gray ore boulder with rust specks |
| Gold | `crates/presentation/assets/icons/commodities/Gold.png` | Stacked gold ingots |
| Gems | `crates/presentation/assets/icons/commodities/Gems.png` | Teal brilliant-cut gem with facets |
| Oil | `crates/presentation/assets/icons/commodities/Oil.png` | Dark drum with gold drop emblem |
| Lumber | `crates/presentation/assets/icons/commodities/Lumber.png` | Stack of three cut planks |
| Steel | `crates/presentation/assets/icons/commodities/Steel.png` | Pyramid of gray ingots |
| Fabric | `crates/presentation/assets/icons/commodities/Fabric.png` | Layered crimson draped cloth |
| Paper | `crates/presentation/assets/icons/commodities/Paper.png` | Parchment scroll with text lines |
| CannedFood | `crates/presentation/assets/icons/commodities/CannedFood.png` | Tin can with red label band |
| Arms | `crates/presentation/assets/icons/commodities/Arms.png` | Flintlock pistol, side view |
| Furniture | `crates/presentation/assets/icons/commodities/Furniture.png` | Wooden chair, side view |
| Clothing | `crates/presentation/assets/icons/commodities/Clothing.png` | Navy frock coat with brass buttons |
| Hardware | `crates/presentation/assets/icons/commodities/Hardware.png` | Eight-tooth gear wheel |

## civilians/ (21)

Each civilian is a worker **figure** (head, torso, role hat) shown with the
tool of their trade — not the tool alone. Every civilian also has two action
frames — `<Name>Work1.png` (mid-swing) and `<Name>Work2.png` (strike: pick
hitting rock, axe biting the log, lasso overhead, …). While a civilian works
on a tile the map marker plays the three images ping-pong
(rest → mid → strike → mid; `WorkFrameAnim` in `map/markers.rs`).

| Name | Output | Pictogram |
|------|--------|-----------|
| Prospector | `crates/presentation/assets/icons/civilians/Prospector.png` | Worker in a wide-brim hat holding a gold pan with nuggets |
| Miner | `crates/presentation/assets/icons/civilians/Miner.png` | Worker in a lamped helmet shouldering a pickaxe |
| Farmer | `crates/presentation/assets/icons/civilians/Farmer.png` | Worker in a straw hat with a pitchfork |
| Rancher | `crates/presentation/assets/icons/civilians/Rancher.png` | Worker in a cowboy hat with a coiled lasso |
| Forester | `crates/presentation/assets/icons/civilians/Forester.png` | Worker in a green cap shouldering an axe |
| Driller | `crates/presentation/assets/icons/civilians/Driller.png` | Worker in an orange hard hat beside an oil derrick |
| Engineer | `crates/presentation/assets/icons/civilians/Engineer.png` | Worker in a blue hard hat holding a wrench, with a gear |

## units/ (6)

| Name | Output | Pictogram |
|------|--------|-----------|
| Infantry | `crates/presentation/assets/icons/units/Infantry.png` | Crossed muskets with bayonets |
| Cavalry | `crates/presentation/assets/icons/units/Cavalry.png` | Gold horseshoe over a saber |
| Artillery | `crates/presentation/assets/icons/units/Artillery.png` | Field cannon on spoked wheel |
| Special | `crates/presentation/assets/icons/units/Special.png` | Powder keg with lit fuse (sapper) |
| General | `crates/presentation/assets/icons/units/General.png` | Bicorne hat with cockade |
| Army | `crates/presentation/assets/icons/units/Army.png` | Swallow-tail standard on pole |

## ships/ (13)

| Name | Output | Pictogram |
|------|--------|-----------|
| Trader | `crates/presentation/assets/icons/ships/Trader.png` | Small single-mast sloop |
| Indiaman | `crates/presentation/assets/icons/ships/Indiaman.png` | Broad three-mast square-rigged merchant |
| Clipper | `crates/presentation/assets/icons/ships/Clipper.png` | Sleek dark hull, raked masts, stay sails |
| Paddlewheeler | `crates/presentation/assets/icons/ships/Paddlewheeler.png` | Side paddle wheel, funnel and smoke |
| Freighter | `crates/presentation/assets/icons/ships/Freighter.png` | Steel steamer with derricks and funnel |
| Frigate | `crates/presentation/assets/icons/ships/Frigate.png` | Three-mast warship, single gunport row |
| ShipOfTheLine | `crates/presentation/assets/icons/ships/ShipOfTheLine.png` | Tall hull, two gunport rows, full sails |
| Raider | `crates/presentation/assets/icons/ships/Raider.png` | Low steam hull with ram bow and gun |
| Ironclad | `crates/presentation/assets/icons/ships/Ironclad.png` | Low armored casemate with gun stub |
| AdvancedIronclad | `crates/presentation/assets/icons/ships/AdvancedIronclad.png` | Two gold-banded funnels, long deckhouse, stern turret, gold hull stripe |
| ArmouredCruiser | `crates/presentation/assets/icons/ships/ArmouredCruiser.png` | Long hull, two funnels, two masts |
| Dreadnought | `crates/presentation/assets/icons/ships/Dreadnought.png` | Battleship, fore and aft turrets, tripod mast |
| Battlecruiser | `crates/presentation/assets/icons/ships/Battlecruiser.png` | Sleek battleship with three funnels |

## infrastructure/ (6)

| Name | Output | Pictogram |
|------|--------|-----------|
| Railroad | `crates/presentation/assets/icons/infrastructure/Railroad.png` | Receding rail track with sleepers |
| Depot | `crates/presentation/assets/icons/infrastructure/Depot.png` | Warehouse with red roof over track |
| Port | `crates/presentation/assets/icons/infrastructure/Port.png` | Quay crane lowering a crate |
| Fort | `crates/presentation/assets/icons/infrastructure/Fort.png` | Crenellated stone tower with gate |
| Capital | `crates/presentation/assets/icons/infrastructure/Capital.png` | Gold five-point star |
| Capitol | `crates/presentation/assets/icons/infrastructure/Capitol.png` | Domed classical building with columns |

## diplomacy/ (8)

| Name | Output | Pictogram |
|------|--------|-----------|
| Consulate | `crates/presentation/assets/icons/diplomacy/Consulate.png` | Small house with red pennant |
| Embassy | `crates/presentation/assets/icons/diplomacy/Embassy.png` | Columned building with national flag |
| NonAggressionPact | `crates/presentation/assets/icons/diplomacy/NonAggressionPact.png` | Diagonal clasped handshake between two sleeved arms |
| Alliance | `crates/presentation/assets/icons/diplomacy/Alliance.png` | Two crossed flags |
| War | `crates/presentation/assets/icons/diplomacy/War.png` | Burning torch |
| Peace | `crates/presentation/assets/icons/diplomacy/Peace.png` | White dove with olive sprig |
| Grant | `crates/presentation/assets/icons/diplomacy/Grant.png` | Tied money sack with gold coin |
| BreakTreaty | `crates/presentation/assets/icons/diplomacy/BreakTreaty.png` | Document torn in two, wax seal |

## ui/ (8)

| Name | Output | Pictogram |
|------|--------|-----------|
| Anchor | `crates/presentation/assets/icons/ui/Anchor.png` | Classic navy anchor with stock and flukes |
| Swords | `crates/presentation/assets/icons/ui/Swords.png` | Two crossed straight swords (legacy; map now uses tents) |
| Treasury | `crates/presentation/assets/icons/ui/Treasury.png` | Gold coin with crown emboss |
| Workers | `crates/presentation/assets/icons/ui/Workers.png` | Two capped worker busts |
| FreightCar | `crates/presentation/assets/icons/ui/FreightCar.png` | Railway boxcar on rail |
| Science | `crates/presentation/assets/icons/ui/Science.png` | Erlenmeyer flask with teal liquid |
| News | `crates/presentation/assets/icons/ui/News.png` | Folded newspaper with masthead |
| Tent | `crates/presentation/assets/icons/ui/Tent.png` | Canvas A-frame tent with red pennant (army encampment marker, 1–4 per capital) |

## terrain/ (7)

Per-tile terrain motifs layered over the color fill in Terrain map mode, so
each tile reads as art (mountains, forest, …) rather than a flat tint.

| Name | Output | Pictogram |
|------|--------|-----------|
| Mountain | `crates/presentation/assets/icons/terrain/Mountain.png` | Two snow-capped grey peaks |
| Hills | `crates/presentation/assets/icons/terrain/Hills.png` | Two rounded green mounds |
| Forest | `crates/presentation/assets/icons/terrain/Forest.png` | Pair of pine trees |
| Swamp | `crates/presentation/assets/icons/terrain/Swamp.png` | Murky water pool with reeds |
| Desert | `crates/presentation/assets/icons/terrain/Desert.png` | Sun over a dune with a cactus |
| Tundra | `crates/presentation/assets/icons/terrain/Tundra.png` | Snowfield with a bare shrub and snowflake |
| Grassland | `crates/presentation/assets/icons/terrain/Grassland.png` | Tufts of grass blades |

## ground/ (8)

Seamlessly tileable ground textures (authored in
`pixel-src/ground.py`), repeated across the merged tile-fill meshes with
world-aligned UVs. Terrain map mode uses them for land; the sea texture is
used in every map mode. Unlike the icon groups these fill all 32×32 pixels
— edit them only with tileability in mind (every motif wraps at the edges).

| Name | Output | Pattern |
|------|--------|---------|
| Grassland | `crates/presentation/assets/icons/ground/Grassland.png` | Meadow green, grass tufts, sparse straw flowers |
| Hills | `crates/presentation/assets/icons/ground/Hills.png` | Tan folds with diagonal contour dashes |
| Forest | `crates/presentation/assets/icons/ground/Forest.png` | Deep green with underbrush clumps |
| Mountain | `crates/presentation/assets/icons/ground/Mountain.png` | Grey scree, crag dashes, snow flecks |
| Desert | `crates/presentation/assets/icons/ground/Desert.png` | Sand with wind-combed ripple dashes |
| Swamp | `crates/presentation/assets/icons/ground/Swamp.png` | Murk green with glinting pools |
| Tundra | `crates/presentation/assets/icons/ground/Tundra.png` | Pale frost with snow drifts |
| Sea | `crates/presentation/assets/icons/ground/Sea.png` | Calm blue with staggered wave crests |

## splash/ (1)

Full-scene pixel art (authored in `pixel-src/splash.py`). Unlike the icon
groups, gen_assets rasterizes this group at the SVG's **native pixel grid**
(no 64×64 downscale); the game upscales with nearest-neighbor at draw time.

| Name | Output | Scene |
|------|--------|-------|
| Title | `crates/presentation/assets/icons/splash/Title.png` | 320×180 dawn landscape for the title screen: sun over the sea, mountains, hill farms, brick mill, steam locomotive, three-mast merchantman. Title/prompt text is engine-side (pixel font), not baked in. |
