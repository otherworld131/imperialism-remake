# 11 — Military — Land Units

## Overview

Army units are assembled in the Armory from workers + arms + money. They range from basic
Regulars and Militia to late-game Mechanised regiments. Units earn medals in combat that
significantly boost effectiveness (4 medals ≈ 2× firepower).

## Checklist

### Unit Data Model
- [ ] `ArmyUnit` entity — type, owner, position (ProvinceId), health (5% increments), medals (0-4+), movement points
- [ ] Unit stats loaded from data files (data-driven, moddable)
- [ ] `ArmyUnitType` — defines base stats: firepower, movement, range, cost, prerequisites, required resources
- [ ] Unit health: 100% → 95% → 90% ... → 5% → destroyed
- [ ] Medal system: each medal increases firepower proportionally (4 medals ≈ 2× base)
- [ ] Medal holders recover health faster between turns
- [ ] Unit tests: health tracking, medal firepower bonus calculation

### Infantry Units
- [ ] **Militia/Minutemen** — immovable garrison (4 per GP province, 3 per MN province); cannot be created; no maintenance; strong on defense
- [ ] **Regulars** — FP:10, Mov:4, Range:5, Cost:$500; no tech required; 1 worker + 1 arm
- [ ] **Grenadiers** — FP:12, Mov:4, Range:5, Cost:$1,000; no tech required
- [ ] **Rifle Infantry** — FP:15, Mov:4, Range:8, Cost:$3,000; requires Rifled Artillery tech
- [ ] **Guards** — FP:17, Mov:4, Range:8, Cost:$4,000; requires Breech-Loading Rifles
- [ ] **Sharpshooters** — requires Bessemer Converter
- [ ] **Modern Infantry** — requires Machine Guns tech
- [ ] **Machine Gunners** — requires Machine Guns tech
- [ ] **Rangers** — requires Machine Guns tech
- [ ] Unit tests: each unit type has correct stats and prerequisites

### Cavalry Units
- [ ] **Cuirassiers** — FP:15, Mov:9, Range:3, Cost:$500; no tech required; needs horse
- [ ] **Scouts** — FP:10, Mov:11, Range:5, Cost:$2,000; requires Bessemer Converter; needs horse
- [ ] **Carbine Cavalry** — FP:20, Mov:9, Range:5, Cost:$3,500; requires Breech-Loading Rifles; needs horse
- [ ] **Armour** — requires Internal Combustion tech (late game)
- [ ] **Mechanised** — requires Internal Combustion tech (late game)
- [ ] Unit tests: cavalry horse requirement validation

### Artillery Units
- [ ] **Light Artillery (Horse Artillery)** — available from start; one of the stronger early units
- [ ] **Standard Artillery** — available from start
- [ ] **Field Artillery** — FP:17, Mov:6, Range:12, Cost:$5,000; requires Rifled Artillery
- [ ] **Siege Artillery** — FP:30, Mov:3, Range:14, Cost:$5,000; requires Rifled Artillery
- [ ] **Railroad Gun** — improved Siege Artillery; requires Large Artillery; extended range; defeats non-upgraded units
- [ ] **Mobile Artillery** — requires Large Artillery
- [ ] Unit tests: artillery range and firepower calculations

### Special Units
- [ ] **Sapper** — requires Expert Worker; tunnels toward forts to destroy them
  - [ ] Uses half movement points to dig one tunnel space
  - [ ] If stationary: 2 tunnel spaces per turn
  - [ ] "S" key skips all units until next Sapper (tactical battle convenience)
  - [ ] Interruption: skip stops if any unit is attacked
- [ ] **General** — earned as reward (build 6 arms' worth of units); boosts army initiative
  - [ ] Initiative rating depends on General's medals and force composition
  - [ ] Subsequent Generals require progressively more arms
  - [ ] Counts as 1 transport unit
- [ ] Unit tests: sapper tunnel progress, general initiative bonus

### Unit Recruitment
- [ ] Built in the Armory — requires worker (mostly trained) + arms + money
- [ ] Cavalry units additionally require horses
- [ ] Units appear the turn after ordering
- [ ] Maintenance: $25/turn per arm in the unit
- [ ] No food requirement once built
- [ ] Unit tests: recruitment cost validation, maintenance calculation

### Unit Upgrades
- [ ] Most units can be upgraded to a higher-grade version when tech is researched
- [ ] Upgrade retains the soldier and all earned medals
- [ ] Only equipment cost is paid (not full unit cost)
- [ ] Example: Regulars → Rifle Infantry → Modern Infantry
- [ ] Example: Cuirassiers → Carbine Cavalry → Armour
- [ ] Example: Light Artillery → Field Artillery → Mobile Artillery
- [ ] Example: Standard Artillery → Siege Artillery → Railroad Gun
- [ ] Unit tests: upgrade path validation, medal preservation, cost calculation

### Unit Movement
- [ ] Movement measured in hex tiles per turn
- [ ] Movement to adjacent province only (march)
- [ ] Rail transport: move to any connected province (1 unit per 5 freight cars)
- [ ] Amphibious transport via port → beachhead (force size = arms of fleet)
- [ ] Militia/Minutemen: immovable — cannot move at all
- [ ] Unit tests: movement range calculations, rail transport eligibility

### Rewards
- [ ] **General**: Build 6 arms' worth of units → free General + first one includes a free unit
- [ ] **Conquest medal**: Conquering a Minor Nation capital → new army unit starts with 1 medal + statue on Armory
- [ ] **Capitol expansion**: Conquering a Great Power capital → recruitment from 4 provinces per worker → 3
- [ ] Unit tests: reward trigger conditions and effects

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all land unit tests pass
- [ ] **Data validation test**: Load all unit definitions from data files → verify all have valid stats, costs, prerequisites
- [ ] **Integration test**: Build a Regulars unit → verify cost deducted, unit appears next turn, maintenance charged
- [ ] **Integration test**: Unit earns 4 medals → verify firepower ≈ 2× base
- [ ] **Integration test**: Upgrade Regulars → Rifle Infantry → verify medals preserved, only equipment cost charged
- [ ] **Edge case tests**: Recruit with insufficient funds, recruit without prerequisite tech, upgrade with no valid path
