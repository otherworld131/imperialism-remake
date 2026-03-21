# 06 — Town Development & Infrastructure

## Overview

Non-capital provinces evolve from hamlets → villages → towns through industrialization.
Industrialization requires connecting a province to the capital via rail depot or port.
Towns produce materials and goods autonomously (no labor needed) based on local resources
and factory capacity.

## Checklist

### Settlement Progression
- [x] **Hamlet** — initial state; produces raw resources only
- [x] **Village** — intermediate; begins producing when connected and factory built
- [x] **Town** — fully industrialized; maximum production capacity
- [x] 6-turn delay from connection before first materials appear
- [x] Captured Great Power capitals industrialize immediately (no delay)
- [x] Minor Nation capitals never industrialize
- [x] Unit tests for settlement progression timing

### Province Connection
- [x] Province is "connected" if a rail depot on/adjacent to its capital links to national capital
- [x] Alternative: port connection to capital (via sea route)
- [x] Connection validation algorithm — pathfinding through rail network or port chain
- [x] Disconnection on province loss — recalculate connectivity
- [x] Unit tests for connection detection

### Town Production
- [x] Town production requires no labor input
- [x] Base ratio: 2 raw resources → 1 material
- [x] Finished goods: 1/2 of available materials → goods
- [x] Production limited by local factory capacity
- [x] Factory capacity progression: 4, 8, 12, 16, ...
- [x] Factory upgrade cost: materials + 2-turn delay for new capacity
- [ ] Three production chains available per town based on local resources:
  - [x] Timber → Lumber → Furniture
  - [x] Cotton/Wool → Fabric → Clothing
  - [x] Coal + Iron → Steel → Hardware
- [x] Towns with multiple resource types (especially coal + iron) are especially valuable
- [x] Unit tests for town production calculations at each capacity level

### Infrastructure — Railroads
- [x] Railroads connect tiles, enabling resource transport to capital
- [x] Built by Engineer units, one tile per turn
- [x] Cost varies by terrain:
  - [x] Plains/Farm/Forest: $100
  - [x] Desert/Tundra: $100-150
  - [x] Swamp: $300 (requires Iron Railroad Bridge tech)
  - [x] Hills: $200 (requires Compound Steam Engine tech)
  - [x] Mountains: requires Dynamite tech
- [x] Railroads also used for military transport (1 army unit per 5 freight cars)
- [x] Unit tests for railroad construction rules

### Infrastructure — Depots
- [x] Depots are collection points for resources from surrounding tiles
- [x] Built by Engineer, costs $2,000, takes 3 turns
- [x] Must be connected to capital via railroad or port to be useful
- [x] Placing depot on/adjacent to a province capital triggers industrialization
- [x] Unit tests for depot placement and connectivity

### Infrastructure — Ports
- [x] Ports provide sea access for trade and military operations
- [x] Built by Engineer on coastal tiles, costs $3,000, takes 3 turns
- [x] Ports cannot be built on hill terrain
- [x] Ports connect provinces to the capital via sea routes
- [x] Required for overseas trade with minor nations
- [x] Unit tests for port placement validation

### Infrastructure — Forts
- [x] Forts provide defensive bonuses in combat
- [x] Three levels:
  - [x] Level 1: $5,000 (available from start)
  - [x] Level 2: $7,500 (requires Bessemer Converter tech)
  - [x] Level 3: $10,000 (requires Large Artillery tech)
- [x] Built by Engineer, takes 3 turns per level
- [ ] Forts affect tactical battle maps — walls, defensive positions
- [ ] Sappers can tunnel to destroy fort sections
- [x] Heavy artillery can also destroy fort sections
- [x] Unit tests for fort construction and defensive bonus calculations

### Capital City Buildings (8 standard)
- [x] **Armory** — build army units from workers + arms + money
- [x] **Capitol** — recruit immigrants (requires canned food + clothing + furniture)
- [x] **Food Processing** — convert raw food → canned food
- [x] **Railyard** — build freight cars (2 labor + 1 lumber + 1 steel each); manage transport
- [x] **Shipyard** — build merchant ships and warships (resource costs, no money cost)
- [x] **Trade School** — train workers: untrained → trained → expert (uses paper)
- [x] **University** — convert expert workers into specialist civilians (uses paper + money)
- [x] **Warehouse** — display inventory and incoming shipments

### Optional / Unlockable Buildings
- [x] **Mills** (Lumber Mill, Steel Mill, Textile Mill) — process raw → materials
- [x] **Factories** (Furniture, Hardware, Clothing) — process materials → goods
- [x] **Oil Refinery** — process oil (unlocked by Oil Drilling tech)
- [x] **Power Plant** — uses oil for bonuses (unlocked by Oil Drilling tech)
- [x] All expandable: 1 lumber + 1 steel per capacity unit
- [x] Easy difficulty: start with 3 mills (cap 2) + 3 factories (cap 1)
- [x] Harder difficulties: must be built from scratch
- [x] Unit tests for building construction and expansion

### Verification Strategy
- [x] **Unit tests**: `cargo test` — all town, infrastructure, and building tests pass
- [x] **Industrialization test**: Connect depot to province → verify 6-turn delay → materials appear on turn 7
- [x] **Town production test**: Town with factory capacity 8 + 4 timber available → verify 2 lumber + 1 furniture produced
- [x] **Railroad cost test**: Build railroad on each terrain type → verify correct cost and tech prerequisite enforced
- [x] **Fort siege test**: Build level 3 fort → verify combat defense bonus applied; sapper destroys section → bonus reduced
- [x] **Building expansion test**: Expand mill from cap 2 → 4 → verify cost = 2 lumber + 2 steel, 2-turn delay
- [x] **Connectivity test**: Province connected via port → rail severed → verify province still connected via sea route
- [x] **Captured capital test**: Capture Great Power capital → verify immediate industrialization (no 6-turn delay)
- [x] **Minor capital test**: Capture Minor Nation capital → verify it does NOT industrialize
