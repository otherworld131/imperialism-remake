# 14 — Nations & Scenarios

## Overview

The game supports random maps with 7 fictional Great Powers + 16 Minor Nations, and
historical scenarios with real European powers. Nations are data-driven and moddable.

## Checklist

### Nation Data Model
- [ ] `Nation` entity — ID, name, color, type (GreatPower/MinorNation), provinces list, capital province
- [ ] Nation data loaded from definition files (JSON/YAML)
- [ ] Fixed vs. Random naming option (computer nations get standard or procedural names)
- [ ] Capital province auto-named: nation name + "City"
- [ ] Each Great Power starts with 8 provinces
- [ ] Each Minor Nation starts with 4 provinces
- [ ] Unit tests: nation creation, naming rules

### Standard Great Powers (Random Game)
- [ ] **Deneb** (Yellow) — provinces: Banlingen, Feolin, Gairloch, Girvan, Lochinvar, Lochmaddy, Mallaig, Nairn
- [ ] **Devron** (Orange) — provinces: Anza, Lopez, Moraga, Pacheco, Rivera, Taraval, Ulloa, Wawona
- [ ] **Haxaco** (Light Blue) — provinces: Hackensack, Hopatcong, Peapack, Piscataway, Secaucus, Teaneck, Watchung, Weehawken
- [ ] **Kem** (Red) — provinces: Hukchi, Kara, Koryak, Latev, Narvik, Tiksi, Totyev, Tromso
- [ ] **Ordune** (Green) — provinces: Banburn, Brigadune, Dunbar, Dundee, Dunham, Dunlap, Dunmore, Oban
- [ ] **Patagon** (Purple) — provinces: Callisto, Faliro, Kailithea, Kifisia, Patisia, Pereus, Perissos, Petralona
- [ ] **Zimm** (Blue) — provinces: Bergen, Essex, Monmouth, Morris, Nassau, Passaic, Sussex, Warren
- [ ] Unit tests: all 7 nations have correct province counts and colors

### Standard Minor Nations (16)
- [ ] Bruhr, Dedge, Hurshen, Idolon, Issa, Kathay, Kessel, Loke, Manx, Pont, Pram, Sindel, Twelt, Wodan, Zazi, Zinlu
- [ ] Each with 4 assigned provinces
- [ ] Unit tests: 16 nations, 4 provinces each

### Historical Scenarios
- [ ] **1815 scenario** — post-Napoleonic Europe
- [ ] **1820 scenario**
- [ ] **1848 scenario** — Year of Revolutions
- [ ] **1882 scenario** — Scramble for Africa era
- [ ] Historical Great Powers: Britain, Spain, France, Netherlands, Prussia, Germany, Austria-Hungary, Sardinia, Italy, Russia, Ottoman Empire (availability varies by scenario)
- [ ] Scenario files define: start year, available nations, map layout, starting resources, pre-researched techs
- [ ] Difficulty ratings per nation per scenario (some nations are harder to play)
- [ ] Unit tests: scenario loading and validation

### Scenario Data Format
- [ ] Define schema for scenario files (JSON/YAML)
- [ ] Fields: name, start_year, description, map_data, nations[], starting_resources{}, starting_techs[], difficulty_ratings{}
- [ ] Map data: fixed terrain layout (not randomly generated)
- [ ] Nation starting conditions: treasury, units, buildings, pre-built infrastructure
- [ ] Validation: all referenced nation IDs exist, province counts correct, tech IDs valid
- [ ] Unit tests: schema validation, required fields present

### Starting Conditions (Random Game)
- [ ] Starting civilians: prospectors, miners, engineers, possibly farmers (varies by difficulty)
- [ ] One warship in nearest sea zone
- [ ] Easy/Introductory: 6 pre-placed processing buildings; program selects capital location
- [ ] Normal+: player selects capital on coast or river; food requirement: ideal 3 grain + 2 fruit + 2 meat, minimum 2+1+1
- [ ] Starting treasury varies by difficulty
- [ ] Starting warehouse contents vary by difficulty
- [ ] Mineral resource density varies by difficulty (more minerals on easier settings)
- [ ] Unit tests: starting condition generation per difficulty level

### Nation Selection Criteria (for UI guidance)
- [ ] Geographic shape: circular nations enable faster railroad expansion
- [ ] Minor Nation adjacency: 2+ neighbors preferred
- [ ] Great Power isolation: avoid sharing a continent
- [ ] Terrain accessibility: reachable without impassable terrain
- [ ] Resource availability: forests, hills/mountains, cotton/wool sources
- [ ] These are hints for the player — not enforced rules

### Difficulty Levels (5)
- [ ] **Introductory** — easiest, program picks capital, extra starting resources
- [ ] **Easy** — simplified, program picks capital
- [ ] **Normal** — human and AI on almost equal footing
- [ ] **Hard** — AI advantages
- [ ] **Nigh-On Impossible (NOI)** — for expert players
- [ ] Difficulty affects: starting cash, warehouse contents, mineral density, capital selection, AI bonuses
- [ ] Tutorial mode available separately
- [ ] Unit tests: each difficulty level applies correct modifiers

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all nation/scenario tests pass
- [ ] **Data validation**: Load all nation definitions → verify counts, colors, province assignments
- [ ] **Scenario test**: Load each historical scenario → verify it produces a valid game state
- [ ] **Map key test**: Generate 10 random maps from known keys → verify reproducibility (same key = same map)
- [ ] **Difficulty test**: Generate games at each difficulty → verify starting conditions match spec
- [ ] **Smoke test**: Start a game with each Great Power → no errors, valid initial state
