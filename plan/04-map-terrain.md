# 04 — Map & Terrain System

## Overview

The game world is a hex-tiled map divided into provinces owned by nations. The map is fully
visible from game start (no fog of war for terrain — but mineral deposits under mountains,
hills, deserts, swamps, and tundra are hidden until prospected).

## Checklist

### Hex Map Data Structure
- [x] `HexMap` struct — stores all tiles indexed by `HexCoord`
- [x] Efficient spatial lookup (dictionary or 2D array with offset mapping)
- [x] `GetTile(HexCoord)` — O(1) access
- [x] `GetTilesInProvince(ProvinceId)` — returns all tiles belonging to a province
- [x] `GetAdjacentTiles(HexCoord)` — returns 6 neighbors (handles map edges)
- [ ] `GetTilesInRange(HexCoord, int radius)` — BFS within radius
- [x] Map boundary handling — tiles outside bounds return `None`
- [ ] Sea tiles — separate from land, form sea zones for naval operations
- [x] Unit tests for map queries

### Terrain Types (14 land + sea)
- [x] `DryPlains` — produces 1 grain, no worker improvement possible
- [x] `OpenRange` — produces livestock
- [x] `HorseRanch` — produces horses
- [x] `Plantation` — produces cotton, improvable by Farmer (levels 1-3)
- [x] `Farm` — produces grain, improvable by Farmer (levels 1-3)
- [x] `Orchard` — produces fruit, improvable by Farmer (levels 1-3)
- [x] `FertileHills` — produces wool, improvable by Rancher (levels 1-3)
- [x] `BarrenHills` — may contain coal/iron (hidden until prospected), improvable by Miner
- [x] `Mountain` — may contain coal/iron/gold/gems (hidden), improvable by Miner; blocks movement until Dynamite
- [x] `HardwoodForest` — produces 1 timber, improvable by Forester (levels 1-3)
- [x] `ScrubForest` — produces 1 timber, NOT improvable
- [x] `Swamp` — may contain oil (hidden), railroad requires Iron Railroad Bridge tech
- [x] `Desert` — may contain oil (hidden), improvable by Driller after Oil Drilling tech
- [x] `Tundra` — may contain oil (hidden), improvable by Driller after Oil Drilling tech
- [x] `Sea` — navigable by ships, divided into sea zones
- [x] Unit tests for terrain resource yields at each level

### Tile State
- [x] `Tile.Terrain` — immutable terrain type
- [x] `Tile.ResourceDeposit` — hidden mineral (`Option`, revealed by prospecting)
- [x] `Tile.ImprovementLevel` — 0 (unimproved) through 3
- [x] `Tile.Infrastructure` — flags: HasRailroad, HasDepot, HasPort, HasFort (with level)
- [x] `Tile.AssignedCivilian` — reference to civilian working this tile (`Option`)
- [x] `Tile.ProvinceId` — which province this tile belongs to
- [x] `Tile.IsCapital` — whether this is a province capital tile
- [x] Resource output calculation: `Tile.CalculateYield()` based on terrain + level
- [x] Unit tests for yield calculations at all levels

### Province System
- [x] `Province` entity — collection of contiguous tiles
- [x] Province has exactly one capital tile
- [x] Province capital naming: nation name + "City" for national capitals
- [x] Province ownership — tracks current controlling nation
- [x] Garrison — immovable Militia/Minutemen (4 for Great Powers, 3 for Minor Nations)
- [x] Province industrialization state (hamlet → village → town)
- [ ] Province connectivity check — is it connected to the national capital via transport?
- [x] Unit tests for province mechanics

### Sea Zones
- [ ] Sea tiles grouped into named sea zones
- [ ] Ships operate at the sea-zone level, not individual tiles
- [ ] Sea zone adjacency graph for naval movement
- [ ] Coastal provinces linked to adjacent sea zones
- [ ] Unit tests for sea zone routing

### Map Generation (Random Maps)
- [x] Seed-based deterministic generation (reproducible via "map key" string)
- [x] Map key → seed conversion (case-sensitive, up to 32 characters)
- [x] Generate landmasses with varied terrain distribution
- [x] Place 7 Great Power homelands (each with 8 provinces)
- [x] Place 16 Minor Nations (each with 4 provinces)
- [x] Distribute terrain types according to balance rules
- [x] Place hidden mineral deposits under hills/mountains/swamp/desert/tundra
- [ ] Ensure each Great Power has viable starting conditions (food, timber, minerals)
- [ ] Generate sea zones between landmasses
- [ ] Place coastal features and ports
- [ ] Validate map: all provinces contiguous, all nations reachable by sea
- [x] Unit tests: generated map satisfies invariants
- [ ] Property-based tests: random seeds always produce valid maps

### Map Rendering Data (provided to Presentation layer)
- [ ] `MapRenderer` trait — domain provides data, presentation renders
- [ ] Tile sprite/asset mapping based on terrain type + improvement level
- [ ] Province border rendering data
- [ ] Nation color overlays
- [ ] Fog/reveal state for mineral deposits
- [ ] Minimap data extraction

### Verification Strategy
- [x] **Unit tests**: `cargo test` — all map, terrain, province, sea zone tests pass
- [x] **Map generation determinism**: Generate map from key "TestKey123" 10 times → identical output every time
- [ ] **Map invariant test**: Generate 50 random maps → every map has: 7 GPs × 8 provinces, 16 MNs × 4 provinces, all provinces contiguous, all nations sea-reachable
- [x] **Terrain yield test**: For each terrain type at each improvement level → verify output matches spec
- [ ] **Province connectivity test**: Build railroad network → verify `IsConnected()` returns true for connected provinces, false for disconnected
- [x] **Prospecting test**: Prospect a mountain tile → verify hidden mineral revealed correctly
- [ ] **Sea zone routing test**: Ship in zone A → destination zone C → verify valid route through intermediate zones
