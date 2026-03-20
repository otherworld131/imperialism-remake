# 12 — Military — Naval Units

## Overview

Ships are built in the Shipyard at no monetary cost but with resource requirements.
They split into merchant ships (cargo transport) and warships (combat, blockade, escort,
beachhead). Naval battles are always resolved by AI.

## Checklist

### Ship Data Model
- [x] `Ship` entity — type, owner, sea zone position, health (hull points), speed
- [ ] Ship stats loaded from data files (data-driven, moddable)
- [x] `ShipType` — defines: firepower, range, armor, hull, speed, cargo capacity, resource costs
- [x] No maintenance cost for any ship type
- [ ] Ships become available the turn after ordering
- [x] Unit tests: ship creation with correct stats

### Merchant Ships
- [x] **Trader** — FP:0, Armor:0, Hull:25, Cargo:2; costs 2 fabric + 4 lumber; no tech
- [x] **Indiaman** — FP:0, Armor:5, Hull:40, Cargo:4; costs 3 fabric + 7 lumber; no tech
- [x] **Clipper** — FP:0, Armor:0, Hull:25, Cargo:4; costs 2 fabric + 6 lumber; requires Streamlined Hulls
- [x] **Paddlewheeler** — FP:0, Armor:5, Hull:35, Cargo:8; costs 6 lumber + 2 steel + 10 coal; requires Paddlewheels
- [x] **Freighter** — requires Marine Engineering (late game, replaces older merchants)
- [ ] Merchant ships not visible on map; appear in battle reports when engaged
- [ ] Each cargo hold carries 1 traded item
- [x] Some merchant ships have armor/speed advantages for surviving blockades
- [x] Unit tests: cargo capacity calculations, construction cost validation

### Warships
- [x] **Frigate** — FP:3, Range:5, Armor:10, Hull:35, Speed:4; costs 2 fabric + 5 lumber + 2 arms; no tech
- [x] **Ship-of-the-Line** — FP:6, Range:6, Armor:20, Hull:65, Speed:3; costs 3 fabric + 8 lumber + 5 arms; no tech
- [x] **Raider** — FP:3, Range:7, Armor:20, Hull:30, Speed:7; costs 6 lumber + 3 arms + 10 coal; requires Paddlewheels
- [x] **Ironclad** — requires Advanced Iron Working (obsoletes some earlier ships)
- [x] **Advanced Ironclad** — requires Steel Armour Plate (obsoletes Ship-of-the-Line)
- [x] **Armoured Cruiser** — requires Marine Engineering (obsoletes Frigates)
- [x] **Dreadnought** — requires Improved Range-Finding (obsoletes Ironclads)
- [x] **Battlecruiser** — requires Improved Range-Finding
- [x] Unit tests: warship stats and prerequisite validation

### Ship Construction
- [x] Built in Shipyard — no money cost, only resources
- [x] Early ships use fabric (sails) + lumber
- [x] Later ships substitute fabric with coal + steel (steam power)
- [x] 2-3 ship models available at game start; others unlock via tech
- [ ] Ship usable the turn after ordering
- [ ] Unit tests: resource cost deduction, availability timing

### Naval Operations
- [ ] **Move** — warship moves between adjacent sea zones
- [ ] **Patrol** — warship attacks enemies encountered in its sea zone
- [ ] **Blockade** — warship intercepts enemy merchant ships in the sea zone
- [ ] **Escort** — warship protects friendly merchant ships from blockade/patrol
- [ ] **Beachhead** — warships establish landing zone on hostile coastline
  - [ ] Landing force size = total arms used to build all ships in the beachhead fleet
  - [ ] Troops can be teleported from friendly ports to the beachhead
- [ ] **Reconnaissance** — estimate enemy ground forces in adjacent coastal provinces
- [ ] Unit tests: each operation type's rules and constraints

### Naval Combat Resolution
- [ ] All naval battles resolved by AI (never player-controlled)
- [x] Combat factors: firepower, range, armor, hull, speed
- [x] Damage applied to hull; ship destroyed when hull reaches 0
- [ ] Battle results reported to player
- [ ] Unit tests: naval combat resolution algorithm

### Obsolescence
- [x] Technologies can obsolete older ship classes
- [x] Steel Armour Plate obsoletes Ship-of-the-Line
- [x] Marine Engineering obsoletes Frigates
- [x] Improved Range-Finding obsoletes Ironclads
- [x] Obsolete ships still function but are severely outclassed
- [x] Unit tests: obsolescence flag set correctly on tech research

### Rewards
- [ ] **Admiral** — build 5 Ships-of-the-Line → free Admiral + free Ship-of-the-Line
- [ ] **Free Clippers** — establishing first colony → free Clipper ships + statue (even without tech)
- [ ] Unit tests: naval reward trigger conditions

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all naval tests pass
- [x] **Data validation**: Load ship definitions → verify all stats, costs, prerequisites
- [ ] **Integration test**: Build a Frigate → verify resources deducted, ship appears next turn
- [ ] **Integration test**: Blockade scenario → verify merchant ships intercepted, trade disrupted
- [ ] **Integration test**: Beachhead → verify landing force size matches fleet arms total
- [ ] **Combat simulation**: Run 100 naval battles with known fleets → verify outcomes are reasonable
