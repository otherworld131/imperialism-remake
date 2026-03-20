# 23 — Modding & Data-Driven Design

> **STATUS: DEFERRED** — Not critical for initial release. Core game data is already
> partially data-driven (tech tree, unit stats in code). Full modding support (RON files,
> Lua scripts, mod loading) deferred to post-MVP.

## Overview

The original supported custom scenarios and map editors. The remake should be deeply
data-driven so that modders can alter units, techs, buildings, nations, scenarios,
and balance without code changes.

## Checklist

### Data-Driven Architecture — RON/JSON (Static) + Lua (Dynamic)

Two complementary data layers:
- **RON/JSON** for static definitions (stats, costs, names, structure) — parsed by `serde`
- **Lua scripts** for dynamic logic (tech effects, AI behavior, scenario events, mod hooks) — executed by `mlua`

#### Static Data (RON)
- [ ] All game entity stats/costs loaded from RON files (not hardcoded)
- [ ] Schema validation via `schemars` + `jsonschema` crates
- [ ] Data files organized by domain:
  - [ ] `data/definitions/units.ron` — army unit types with stats, costs, prerequisites
  - [ ] `data/definitions/ships.ron` — ship types with stats, costs, prerequisites
  - [ ] `data/definitions/technologies.ron` — tech tree structure (names, costs, prereqs, year ranges)
  - [ ] `data/definitions/buildings.ron` — building types, capacities, costs
  - [ ] `data/definitions/terrain.ron` — terrain types, yields, improvement rules
  - [ ] `data/definitions/resources.ron` — resource types, production chain ratios
  - [ ] `data/definitions/nations.ron` — nation definitions, provinces, colors
  - [ ] `data/definitions/diplomacy.ron` — treaty types, relationship modifiers
  - [ ] `data/definitions/difficulty.ron` — difficulty level parameters
  - [ ] `data/definitions/rewards.ron` — reward triggers and effects

#### Dynamic Logic (Lua)
- [ ] `scripts/tech/{tech_id}.lua` — per-technology effect scripts
- [ ] `scripts/ai/{personality}.lua` — AI strategy scripts (balanced, aggressive, diplomatic, economic)
- [ ] `scripts/scenarios/{scenario_id}.lua` — scenario event triggers, custom objectives
- [ ] `scripts/mods/{mod_id}/init.lua` — mod entry points with hook registrations
- [ ] Documented callback API: `on_turn_start`, `on_turn_end`, `on_combat_end`, `on_province_conquered`, `on_tech_researched`, `on_trade_resolved`, …
- [ ] Lua sandbox enforced: no `os`, `io`, `loadfile`, `require` — only game API + pure computation

#### Loading & Validation
- [ ] Schema files for each RON definition type
- [ ] Data validation on load — reject invalid/incomplete definitions with clear error messages
- [ ] Lua scripts validated on load — syntax check + sandbox policy check
- [ ] Hot-reload for both RON files and Lua scripts during development (file watcher)
- [ ] Unit tests: load all default RON definitions → validate against schemas
- [ ] Unit tests: load all default Lua scripts → execute without errors in sandbox

### Scenario System
- [ ] Scenario file format: map + nation setup + starting conditions + objectives
- [ ] Scenario includes: fixed map (terrain layout), nation assignments, starting resources, pre-researched techs, starting units, starting buildings, starting infrastructure
- [ ] Scenario difficulty ratings per nation
- [ ] Scenario description/flavor text
- [ ] Multiple scenarios bundled with the game (1815, 1820, 1848, 1882)
- [ ] Custom scenarios loadable from user directory
- [ ] Unit tests: scenario loading and validation

### Map Editor
- [ ] Tool for creating custom maps
- [ ] Hex grid with terrain painting
- [ ] Province boundary drawing
- [ ] Nation assignment to provinces
- [ ] Resource placement (visible and hidden deposits)
- [ ] Sea zone definition
- [ ] Export to scenario format
- [ ] Import existing maps for modification
- [ ] Validation: ensures map meets game requirements (contiguous provinces, valid nation counts, etc.)

### Scenario Editor
- [ ] Build on top of map editor
- [ ] Set starting year and tech availability
- [ ] Configure per-nation starting conditions (treasury, units, buildings, techs)
- [ ] Set difficulty ratings per nation
- [ ] Add scenario description and metadata
- [ ] Playtest from editor (launch game with scenario)

### Mod Loading
- [ ] Mod directory: `mods/` folder in user data directory
- [ ] Each mod is a subfolder with a `mod.json` manifest:
  - [ ] Name, version, author, description
  - [ ] List of files overridden/added
  - [ ] Compatibility version range
- [ ] Mod load order: base data → mods applied in order (later mods override earlier)
- [ ] Merge strategy: mods can add new entries or override existing entries by ID
- [ ] Mod conflicts detected and reported
- [ ] Enable/disable mods from game menu
- [ ] Unit tests: mod loading, override resolution, conflict detection

### Localization as Data
- [ ] All user-facing strings in localization files (not hardcoded)
- [ ] `data/localization/{locale}.json` — string tables keyed by ID
- [ ] Default: English (`en.json`)
- [ ] Mods can add/override localization strings
- [ ] Localization placeholder support: `"You conquered {province_name}"`
- [ ] Unit tests: all string IDs referenced in code exist in the default locale file

### Verification Strategy
- [ ] **Unit tests**: All data loading and validation tests pass
- [ ] **Schema test**: Every default data file passes its JSON schema validation
- [ ] **Mod test**: Create a test mod that overrides unit stats → load game → verify overridden values active
- [ ] **Scenario test**: Create a custom scenario in the editor → export → load in game → plays correctly
- [ ] **Localization test**: Switch locale → verify all strings display correctly, no missing IDs
- [ ] **Completeness test**: Grep all string references in code → verify 100% coverage in default locale
- [ ] **Validation test**: Feed intentionally broken data files → verify clear error messages, no crashes
