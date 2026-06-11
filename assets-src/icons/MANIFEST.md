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

Shared style: flat fills, dark outline `#2a2418` (stroke 2.5 at 64px), warm
parchment-era palette (parchment `#f2ead6`, wood `#8a5a2b`, brass/gold
`#d9a441`/`#e3b341`, navy `#2c4a66`, steel `#9aa0a6`, coal `#3a3530`,
brick `#a33b2e`).

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

## civilians/ (7)

| Name | Output | Pictogram |
|------|--------|-----------|
| Prospector | `crates/presentation/assets/icons/civilians/Prospector.png` | Gold pan with nuggets, pick behind |
| Miner | `crates/presentation/assets/icons/civilians/Miner.png` | Crossed pickaxe and spade |
| Farmer | `crates/presentation/assets/icons/civilians/Farmer.png` | Pitchfork beside wheat stalk |
| Rancher | `crates/presentation/assets/icons/civilians/Rancher.png` | Coiled lasso with open loop |
| Forester | `crates/presentation/assets/icons/civilians/Forester.png` | Axe embedded in tree stump |
| Driller | `crates/presentation/assets/icons/civilians/Driller.png` | Oil derrick tower with oil drop |
| Engineer | `crates/presentation/assets/icons/civilians/Engineer.png` | Crossed hammer and spanner over hard hat |

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
| AdvancedIronclad | `crates/presentation/assets/icons/ships/AdvancedIronclad.png` | Ironclad with turret and two funnels |
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
| NonAggressionPact | `crates/presentation/assets/icons/diplomacy/NonAggressionPact.png` | Handshake between two sleeved arms |
| Alliance | `crates/presentation/assets/icons/diplomacy/Alliance.png` | Two crossed flags |
| War | `crates/presentation/assets/icons/diplomacy/War.png` | Burning torch |
| Peace | `crates/presentation/assets/icons/diplomacy/Peace.png` | White dove with olive sprig |
| Grant | `crates/presentation/assets/icons/diplomacy/Grant.png` | Tied money sack with gold coin |
| BreakTreaty | `crates/presentation/assets/icons/diplomacy/BreakTreaty.png` | Document torn in two, wax seal |

## ui/ (7)

| Name | Output | Pictogram |
|------|--------|-----------|
| Anchor | `crates/presentation/assets/icons/ui/Anchor.png` | Classic navy anchor with stock and flukes |
| Swords | `crates/presentation/assets/icons/ui/Swords.png` | Two crossed straight swords |
| Treasury | `crates/presentation/assets/icons/ui/Treasury.png` | Gold coin with crown emboss |
| Workers | `crates/presentation/assets/icons/ui/Workers.png` | Two capped worker busts |
| FreightCar | `crates/presentation/assets/icons/ui/FreightCar.png` | Railway boxcar on rail |
| Science | `crates/presentation/assets/icons/ui/Science.png` | Erlenmeyer flask with teal liquid |
| News | `crates/presentation/assets/icons/ui/News.png` | Folded newspaper with masthead |
