# 03 — Core Domain Model

## Overview

The domain core contains all game rules, entities, value objects, aggregates, and domain
services. It has **zero** external dependencies — no framework, no I/O, no UI.

## Checklist

### Coordinate System
- [x] Implement `HexCoord` value object (axial coordinates: q, r)
- [x] Implement cube-coordinate conversion (q, r, s where q + r + s = 0)
- [x] Implement `HexCoord.Neighbors()` — 6 adjacent hex directions
- [x] Implement `HexCoord.Distance(other)` — hex Manhattan distance
- [x] Implement `HexCoord.Ring(radius)` — all hexes at exact distance
- [x] Implement `HexCoord.LineTo(other)` — hex line-drawing algorithm
- [x] Implement `HexCoord.ToPixel()` / `HexCoord.FromPixel()` conversions
- [x] Unit tests for all coordinate operations (≥ 30 cases)

### Core Value Objects
- [x] `PlayerId` — strongly-typed identifier
- [x] `NationId` — strongly-typed identifier
- [x] `ProvinceId` — strongly-typed identifier
- [x] `TileId` — strongly-typed identifier (or use HexCoord directly)
- [x] `TurnNumber` — value object wrapping int, with year/quarter conversion (1815 Q1 = turn 1)
- [x] `Money` — value object, prevents negative-money bugs, arithmetic operators
- [x] `ResourceAmount` — (ResourceType, quantity) pair
- [x] `ResourceType` enum — Timber, Coal, Iron, Cotton, Wool, Grain, Fruit, Livestock, Oil, Gold, Gems
- [x] `MaterialType` enum — Lumber, Steel, Fabric, Paper, Arms, CannedFood
- [x] `GoodsType` enum — Furniture, Clothing, Hardware
- [x] `TerrainType` enum — DryPlains, OpenRange, HorseRanch, Plantation, Farm, Orchard, FertileHills, BarrenHills, Mountain, HardwoodForest, ScrubForest, Swamp, Desert, Tundra, Sea, Capital
- [x] Unit tests for all value objects (equality, immutability, edge cases)

### Entity Base
- [ ] `Entity<TId>` base trait with identity-based equality
- [ ] `AggregateRoot<TId>` base trait with domain event collection
- [ ] `DomainEvent` base struct with timestamp and correlation ID

### Core Entities (Stubs — detailed in later plans)
- [x] `GameState` aggregate root — the top-level game container
- [x] `Nation` entity — name, color, type (GreatPower / MinorNation), provinces, treasury
- [x] `Province` entity — name, owner, tiles collection, capital tile, garrison
- [x] `Tile` entity — HexCoord, terrain, resource, improvement level, infrastructure
- [x] `Unit` entity — type, owner, position, health, medals, movement points
- [x] `Building` entity — type, capacity, upgrades
- [ ] `TechResearch` entity — tech ID, researched flag, turn researched

### Domain Services (Traits)
- [x] `MapGenerator` trait — creates a random map from a seed/key
- [x] `TurnProcessor` trait — orchestrates a full turn resolution
- [x] `CombatResolver` trait — resolves land and naval battles
- [ ] `TradeResolver` trait — resolves trade session offers/bids
- [ ] `DiplomacyResolver` trait — resolves treaty proposals and diplomatic actions
- [ ] `AiDecisionMaker` trait — trait for AI player strategies
- [x] `VictoryChecker` trait — evaluates Council of Governors vote

### Domain Events (Initial Set)
- [x] `TurnStarted(TurnNumber)`
- [x] `TurnEnded(TurnNumber)`
- [x] `TechnologyResearched(NationId, TechId)`
- [x] `WarDeclared(NationId attacker, NationId defender)`
- [x] `TreatyProposed(NationId from, NationId to, TreatyType)`
- [x] `TreatyAccepted / TreatyRejected`
- [x] `ProvinceConquered(ProvinceId, NationId newOwner)`
- [x] `UnitCreated / UnitDestroyed / UnitUpgraded`
- [x] `TradeCompleted(NationId buyer, NationId seller, ...)`
- [x] `BuildingConstructed / BuildingUpgraded`
- [x] `NationEliminatedFromCouncil`
- [x] `VictoryAchieved(NationId winner, VictoryType)`

### Verification Strategy
- [x] **Unit tests**: `cargo test -p domain` — all coordinate, value object, entity, and event tests pass
- [ ] **Coverage check**: Generate coverage report for Domain crate → verify ≥ 90% line coverage
- [ ] **Immutability test**: Attempt to mutate value objects → verify compile-time or runtime errors
- [ ] **Identity test**: Two entities with same ID are equal; two with different IDs are not
- [ ] **Event test**: Publish each domain event type → verify correct handlers fire
- [x] **Compile check**: Domain crate builds with zero external crate dependencies (verified by inspecting `Cargo.toml`)
