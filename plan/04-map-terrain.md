# 04 — Map & Terrain System

## Overview

The game world is a hex-tiled map divided into provinces owned by nations. The map is fully
visible from game start (no fog of war for terrain — but mineral deposits under mountains,
hills, deserts, swamps, and tundra are hidden until prospected).

## Checklist

### Hex Map Data Structure
- [ ] `HexMap` struct — stores all tiles indexed by `HexCoord`
- [ ] Efficient spatial lookup (dictionary or 2D array with offset mapping)
- [ ] `GetTile(HexCoord)` — O(1) access
- [ ] `GetTilesInProvince(ProvinceId)` — returns all tiles belonging to a province
- [ ] `GetAdjacentTiles(HexCoord)` — returns 6 neighbors (handles map edges)
- [ ] `GetTilesInRange(HexCoord, int radius)` — BFS within radius
- [ ] Map boundary handling — tiles outside bounds return `None`
- [ ] Sea tiles — separate from land, form sea zones for naval operations
- [ ] Unit tests for map queries

### Terrain Types (14 land + sea)
- [ ] `DryPlains` — produces 1 grain, no worker improvement possible
- [ ] `OpenRange` — produces livestock
- [ ] `HorseRanch` — produces horses
- [ ] `Plantation` — produces cotton, improvable by Farmer (levels 1-3)
- [ ] `Farm` — produces grain, improvable by Farmer (levels 1-3)
- [ ] `Orchard` — produces fruit, improvable by Farmer (levels 1-3)
- [ ] `FertileHills` — produces wool, improvable by Rancher (levels 1-3)
- [ ] `BarrenHills` — may contain coal/iron (hidden until prospected), improvable by Miner
- [ ] `Mountain` — may contain coal/iron/gold/gems (hidden), improvable by Miner; blocks movement until Dynamite
- [ ] `HardwoodForest` — produces 1 timber, improvable by Forester (levels 1-3)
- [ ] `ScrubForest` — produces 1 timber, NOT improvable
- [ ] `Swamp` — may contain oil (hidden), railroad requires Iron Railroad Bridge tech
- [ ] `Desert` — may contain oil (hidden), improvable by Driller after Oil Drilling tech
- [ ] `Tundra` — may contain oil (hidden), improvable by Driller after Oil Drilling tech
- [ ] `Sea` — navigable by ships, divided into sea zones
- [ ] Unit tests for terrain resource yields at each level

### Tile State
- [ ] `Tile.Terrain` — immutable terrain type
- [ ] `Tile.ResourceDeposit` — hidden mineral (`Option`, revealed by prospecting)
- [ ] `Tile.ImprovementLevel` — 0 (unimproved) through 3
- [ ] `Tile.Infrastructure` — flags: HasRailroad, HasDepot, HasPort, HasFort (with level)
- [ ] `Tile.AssignedCivilian` — reference to civilian working this tile (`Option`)
- [ ] `Tile.ProvinceId` — which province this tile belongs to
- [ ] `Tile.IsCapital` — whether this is a province capital tile
- [ ] Resource output calculation: `Tile.CalculateYield()` based on terrain + level
- [ ] Unit tests for yield calculations at all levels

### Province System
- [ ] `Province` entity — collection of contiguous tiles
- [ ] Province has exactly one capital tile
- [ ] Province capital naming: nation name + "City" for national capitals
- [ ] Province ownership — tracks current controlling nation
- [ ] Garrison — immovable Militia/Minutemen (4 for Great Powers, 3 for Minor Nations)
- [ ] Province industrialization state (hamlet → village → town)
- [ ] Province connectivity check — is it connected to the national capital via transport?
- [ ] Unit tests for province mechanics

### Sea Zones
- [ ] Sea tiles grouped into named sea zones
- [ ] Ships operate at the sea-zone level, not individual tiles
- [ ] Sea zone adjacency graph for naval movement
- [ ] Coastal provinces linked to adjacent sea zones
- [ ] Unit tests for sea zone routing

### Map Generation (Random Maps)
- [ ] Seed-based deterministic generation (reproducible via "map key" string)
- [ ] Map key → seed conversion (case-sensitive, up to 32 characters)
- [ ] Generate landmasses with varied terrain distribution
- [ ] Place 7 Great Power homelands (each with 8 provinces)
- [ ] Place 16 Minor Nations (each with 4 provinces)
- [ ] Distribute terrain types according to balance rules
- [ ] Place hidden mineral deposits under hills/mountains/swamp/desert/tundra
- [ ] Ensure each Great Power has viable starting conditions (food, timber, minerals)
- [ ] Generate sea zones between landmasses
- [ ] Place coastal features and ports
- [ ] Validate map: all provinces contiguous, all nations reachable by sea
- [ ] Unit tests: generated map satisfies invariants
- [ ] Property-based tests: random seeds always produce valid maps

### Map Rendering Data (provided to Presentation layer)
- [ ] `MapRenderer` trait — domain provides data, presentation renders
- [ ] Tile sprite/asset mapping based on terrain type + improvement level
- [ ] Province border rendering data
- [ ] Nation color overlays
- [ ] Fog/reveal state for mineral deposits
- [ ] Minimap data extraction

### Verification Strategy
- [ ] **Unit tests**: `cargo test` — all map, terrain, province, sea zone tests pass
- [ ] **Map generation determinism**: Generate map from key "TestKey123" 10 times → identical output every time
- [ ] **Map invariant test**: Generate 50 random maps → every map has: 7 GPs × 8 provinces, 16 MNs × 4 provinces, all provinces contiguous, all nations sea-reachable
- [ ] **Terrain yield test**: For each terrain type at each improvement level → verify output matches spec
- [ ] **Province connectivity test**: Build railroad network → verify `IsConnected()` returns true for connected provinces, false for disconnected
- [ ] **Prospecting test**: Prospect a mountain tile → verify hidden mineral revealed correctly
- [ ] **Sea zone routing test**: Ship in zone A → destination zone C → verify valid route through intermediate zones
