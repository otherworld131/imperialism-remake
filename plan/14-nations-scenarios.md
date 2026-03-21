# 14 — Nations & Scenarios

## Overview

The game supports random maps with 7 fictional Great Powers + 16 Minor Nations, and
historical scenarios with real European powers. Nations are data-driven and moddable.

## Checklist

### Nation Data Model
- [x] `Nation` entity — ID, name, color, type (GreatPower/MinorNation), provinces list, capital province
- [ ] Nation data loaded from definition files (JSON/YAML)
- [ ] Fixed vs. Random naming option (computer nations get standard or procedural names)
- [x] Capital province auto-named: nation name + "City"
- [x] Each Great Power starts with 8 provinces
- [x] Each Minor Nation starts with 4 provinces
- [x] Unit tests: nation creation, naming rules

### Standard Great Powers (Random Game)
- [x] **Deneb** (Yellow) — provinces: Banlingen, Feolin, Gairloch, Girvan, Lochinvar, Lochmaddy, Mallaig, Nairn
- [x] **Devron** (Orange) — provinces: Anza, Lopez, Moraga, Pacheco, Rivera, Taraval, Ulloa, Wawona
- [x] **Haxaco** (Light Blue) — provinces: Hackensack, Hopatcong, Peapack, Piscataway, Secaucus, Teaneck, Watchung, Weehawken
- [x] **Kem** (Red) — provinces: Hukchi, Kara, Koryak, Latev, Narvik, Tiksi, Totyev, Tromso
- [x] **Ordune** (Green) — provinces: Banburn, Brigadune, Dunbar, Dundee, Dunham, Dunlap, Dunmore, Oban
- [x] **Patagon** (Purple) — provinces: Callisto, Faliro, Kailithea, Kifisia, Patisia, Pereus, Perissos, Petralona
- [x] **Zimm** (Blue) — provinces: Bergen, Essex, Monmouth, Morris, Nassau, Passaic, Sussex, Warren
- [x] Unit tests: all 7 nations have correct province counts and colors

### Standard Minor Nations (16)
- [x] Bruhr, Dedge, Hurshen, Idolon, Issa, Kathay, Kessel, Loke, Manx, Pont, Pram, Sindel, Twelt, Wodan, Zazi, Zinlu
- [x] Each with 4 assigned provinces
- [x] Unit tests: 16 nations, 4 provinces each

### Historical Scenarios
- [x] **1815 scenario** — post-Napoleonic Europe
- [x] **1820 scenario**
- [x] **1848 scenario** — Year of Revolutions
- [x] **1882 scenario** — Scramble for Africa era
- [x] Historical Great Powers: Britain, Spain, France, Netherlands, Prussia, Germany, Austria-Hungary, Sardinia, Italy, Russia, Ottoman Empire (availability varies by scenario)
- [x] Scenario files define: start year, available nations, map layout, starting resources, pre-researched techs
- [x] Difficulty ratings per nation per scenario (some nations are harder to play)
- [x] Unit tests: scenario loading and validation

### Scenario Data Format
- [x] Define schema for scenario files (JSON/YAML)
- [x] Fields: name, start_year, description, map_data, nations[], starting_resources{}, starting_techs[], difficulty_ratings{}
- [ ] Map data: fixed terrain layout (not randomly generated)
- [x] Nation starting conditions: treasury, units, buildings, pre-built infrastructure
- [x] Validation: all referenced nation IDs exist, province counts correct, tech IDs valid
- [x] Unit tests: schema validation, required fields present

### Starting Conditions (Random Game)
- [x] Starting civilians: prospectors, miners, engineers, possibly farmers (varies by difficulty)
- [x] One warship in nearest sea zone
- [x] Easy/Introductory: 6 pre-placed processing buildings; program selects capital location
- [ ] Normal+: player selects capital on coast or river; food requirement: ideal 3 grain + 2 fruit + 2 meat, minimum 2+1+1
- [x] Starting treasury varies by difficulty
- [x] Starting warehouse contents vary by difficulty
- [x] Mineral resource density varies by difficulty (more minerals on easier settings)
- [x] Unit tests: starting condition generation per difficulty level

### Nation Selection Criteria (for UI guidance)
- [x] Geographic shape: circular nations enable faster railroad expansion
- [x] Minor Nation adjacency: 2+ neighbors preferred
- [x] Great Power isolation: avoid sharing a continent
- [x] Terrain accessibility: reachable without impassable terrain
- [x] Resource availability: forests, hills/mountains, cotton/wool sources
- [x] These are hints for the player — not enforced rules

### Difficulty Levels (5)
- [x] **Introductory** — easiest, program picks capital, extra starting resources
- [x] **Easy** — simplified, program picks capital
- [x] **Normal** — human and AI on almost equal footing
- [x] **Hard** — AI advantages
- [x] **Nigh-On Impossible (NOI)** — for expert players
- [x] Difficulty affects: starting cash, warehouse contents, mineral density, capital selection, AI bonuses
- [ ] Tutorial mode available separately
- [x] Unit tests: each difficulty level applies correct modifiers

### Verification Strategy
- [x] **Unit tests**: Run test suite — all nation/scenario tests pass
- [x] **Data validation**: Load all nation definitions → verify counts, colors, province assignments
- [x] **Scenario test**: Load each historical scenario → verify it produces a valid game state
- [x] **Map key test**: Generate 10 random maps from known keys → verify reproducibility (same key = same map)
- [x] **Difficulty test**: Generate games at each difficulty → verify starting conditions match spec
- [x] **Smoke test**: Start a game with each Great Power → no errors, valid initial state
