# 08 — Technology Tree

## Overview

Technologies represent Industrial Revolution discoveries that unlock new units, improvements,
buildings, and capabilities. They become available at irregular intervals and require money
to research. Some have prerequisites.

## Checklist

### Tech Data Model — Rust + Lua
- [x] `Technology` entity (Rust) — ID, name, cost, year range (earliest/latest), prerequisites
- [ ] Tech static data defined in RON (`data/definitions/technologies.ron`) — structure, costs, prereqs, year ranges
- [ ] Tech effects defined in **Lua** (`scripts/tech/{tech_id}.lua`):
  - [ ] Each tech script exports `on_researched(game_api)` — applies effects when researched
  - [ ] Game API exposed to Lua: `game_api:unlock_unit("rifle_infantry")`, `game_api:enable_improvement("farm", 2)`, `game_api:enable_infrastructure("swamp_railroad")`, etc.
  - [ ] Modders add new techs by adding a RON entry + a Lua effect script
- [x] `TechTree` aggregate (Rust) — manages prerequisite graph and availability windows
- [x] `TechTree::get_available(nation_id, turn)` — returns techs available this turn (Rust)
- [ ] Lua scripts are sandboxed — can only call the game API, no file/network/OS access
- [x] Unit tests: prerequisite chain validation (no cycles, all prereqs exist)
- [ ] Unit tests: each default Lua tech script loads and executes against mock game API

### Technology Availability
- [x] Each tech has an earliest and latest year it can appear
- [ ] Random appearance within the year range (seeded by game seed)
- [x] Tech appears in the tech screen only if all prerequisites are met
- [ ] If prerequisites not met, the screen shows remaining prerequisites instead of cost
- [x] Unit tests: tech availability window calculations
- [x] Unit tests: prerequisite filtering

### Complete Technology List

#### Early Era (1815–1825)
- [x] High Pressure Steam Engine — $0(?), enables railroad on desert/farm/forest/plain/tundra ($100-150/tile)
- [x] Seed Drill — $0(?), Farmer improves grain farms and orchards to level 1 ($100/tile)
- [x] Cotton Gin — $1,000, Farmer improves cotton plantations to level 1; prereq for Spinning Jenny
- [x] Iron Railroad Bridge — $1,500, enables swamp railroads ($300/tile); unlocks Forester
- [x] Feed Grasses — $1,500, unlocks Rancher; improves wool farms and livestock ranches to level 1
- [x] Square-Set Timbering — $1,500, Miners upgrade mines to level 2 (4 units output, $1,000/tile)
- [x] Streamlined Hulls — $1,500, unlocks Clipper Ships (no armor, faster, cheaper)

#### Mid Era (1826–1850)
- [x] Spinning Jenny — $3,000, prereqs: Cotton Gin + Feed Grasses; cotton/wool to level 2
- [x] Paddlewheels — $3,000, unlocks Paddlewheelers and Raiders (coal-powered ships)
- [x] Steel Plows — $3,000, prereq: Seed Drill; grain farms and orchards to level 2
- [x] Bessemer Converter — $6,000, level 2 Forts; unlocks Sharpshooters, Scouts; unit conversions
- [x] Compound Steam Engine — $7,000, prereq: Iron Railroad Bridge; hill railroads ($200/tile); hardwood forest level 2
- [x] Breech-Loading Rifles — $12,000, prereq: Bessemer Converter; unlocks Rifle Infantry, Guards, Carbine Cavalry
- [x] Rifled Artillery — $10,000, unlocks Field & Siege Artillery; upgrades existing artillery
- [x] Advanced Iron Working — $12,000, enables Ironclad construction
- [x] Power Loom — $12,000, prereq: Spinning Jenny; cotton/wool to level 3

#### Late Era (1851–1875)
- [x] Mechanical Reaper — $12,000, prereq: Steel Plows; grain farms to level 3
- [x] Commercial Fertilizer — $12,000, prereq: Steel Plows; orchards to level 3
- [x] Oil Drilling — $25,000, unlocks Drillers; level 1 oil production; Oil Refinery + Power Plant
- [x] Barbed Wire — $20,000, prereq: Feed Grasses; livestock ranches to level 2
- [x] Steel Armour Plate — $40,000, prereq: Advanced Iron Working; Advanced Ironclads (obsoletes Ship-of-the-Line)
- [x] Large Artillery — $40,000, prereq: Rifled Artillery; level 3 Forts; Railroad Guns, Mobile Artillery
- [x] Dynamite — $40,000, prereqs: Compound Steam Engine + Square-Set Timbering; mountain railroads; timber/mines to level 3
- [x] Marine Engineering — $40,000, prereq: Steel Armour Plate; Freighters, Armoured Cruisers (obsoletes Frigates)

#### Advanced Era (1879–1898)
- [x] Machine Guns — $100,000, prereq: Breech-Loading Rifles; Modern Infantry, Machine Gunners, Rangers
- [x] Chemistry — $120,000, prereqs: Oil Drilling + Barbed Wire; oil wells level 2; livestock ranches level 3
- [x] Improved Range-Finding — $150,000, prereq: Marine Engineering; Dreadnoughts, Battlecruisers (obsoletes Ironclads)
- [x] Internal Combustion — $150,000, prereq: Chemistry; Armour, Mechanised regiments; oil wells level 3

### Research Mechanics
- [ ] Player selects a technology to research from available list
- [ ] Full cost paid immediately (no incremental research — single purchase)
- [ ] Technology effect applies starting next turn
- [ ] Newspaper reports new technology discoveries
- [ ] Unit tests: research purchase deducts correct money
- [ ] Unit tests: effects correctly unlock after research

### Prerequisite Graph Validation
- [x] Build directed acyclic graph from tech prerequisites
- [x] Validate no circular dependencies
- [x] Validate all referenced prerequisite IDs exist
- [x] Topological sort for display ordering
- [x] Unit tests: graph validation passes for the full tech tree

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all tech tree tests pass
- [ ] **Data validation test**: Load tech definitions from data files → validate all IDs, costs, prereqs, year ranges
- [ ] **Integration test**: Simulate 100 turns of tech research → verify all techs can be researched in correct order
- [ ] **Scenario test**: Start game at year 1815, verify exactly which techs are available; advance to 1830, verify new techs appear
- [ ] **Regression test**: Verify scenario start dates (1815, 1820, 1848, 1882) provide correct starting techs
