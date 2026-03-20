# 03 — Core Domain Model

## Overview

The domain core contains all game rules, entities, value objects, aggregates, and domain
services. It has **zero** external dependencies — no framework, no I/O, no UI.

## Checklist

### Coordinate System
- [ ] Implement `HexCoord` value object (axial coordinates: q, r)
- [ ] Implement cube-coordinate conversion (q, r, s where q + r + s = 0)
- [ ] Implement `HexCoord.Neighbors()` — 6 adjacent hex directions
- [ ] Implement `HexCoord.Distance(other)` — hex Manhattan distance
- [ ] Implement `HexCoord.Ring(radius)` — all hexes at exact distance
- [ ] Implement `HexCoord.LineTo(other)` — hex line-drawing algorithm
- [ ] Implement `HexCoord.ToPixel()` / `HexCoord.FromPixel()` conversions
- [ ] Unit tests for all coordinate operations (≥ 30 cases)

### Core Value Objects
- [ ] `PlayerId` — strongly-typed identifier
- [ ] `NationId` — strongly-typed identifier
- [ ] `ProvinceId` — strongly-typed identifier
- [ ] `TileId` — strongly-typed identifier (or use HexCoord directly)
- [ ] `TurnNumber` — value object wrapping int, with year/quarter conversion (1815 Q1 = turn 1)
- [ ] `Money` — value object, prevents negative-money bugs, arithmetic operators
- [ ] `ResourceAmount` — (ResourceType, quantity) pair
- [ ] `ResourceType` enum — Timber, Coal, Iron, Cotton, Wool, Grain, Fruit, Livestock, Oil, Gold, Gems
- [ ] `MaterialType` enum — Lumber, Steel, Fabric, Paper, Arms, CannedFood
- [ ] `GoodsType` enum — Furniture, Clothing, Hardware
- [ ] `TerrainType` enum — DryPlains, OpenRange, HorseRanch, Plantation, Farm, Orchard, FertileHills, BarrenHills, Mountain, HardwoodForest, ScrubForest, Swamp, Desert, Tundra, Sea, Capital
- [ ] Unit tests for all value objects (equality, immutability, edge cases)

### Entity Base
- [ ] `Entity<TId>` base trait with identity-based equality
- [ ] `AggregateRoot<TId>` base trait with domain event collection
- [ ] `DomainEvent` base struct with timestamp and correlation ID

### Core Entities (Stubs — detailed in later plans)
- [ ] `GameState` aggregate root — the top-level game container
- [ ] `Nation` entity — name, color, type (GreatPower / MinorNation), provinces, treasury
- [ ] `Province` entity — name, owner, tiles collection, capital tile, garrison
- [ ] `Tile` entity — HexCoord, terrain, resource, improvement level, infrastructure
- [ ] `Unit` entity — type, owner, position, health, medals, movement points
- [ ] `Building` entity — type, capacity, upgrades
- [ ] `TechResearch` entity — tech ID, researched flag, turn researched

### Domain Services (Traits)
- [ ] `MapGenerator` trait — creates a random map from a seed/key
- [ ] `TurnProcessor` trait — orchestrates a full turn resolution
- [ ] `CombatResolver` trait — resolves land and naval battles
- [ ] `TradeResolver` trait — resolves trade session offers/bids
- [ ] `DiplomacyResolver` trait — resolves treaty proposals and diplomatic actions
- [ ] `AiDecisionMaker` trait — trait for AI player strategies
- [ ] `VictoryChecker` trait — evaluates Council of Governors vote

### Domain Events (Initial Set)
- [ ] `TurnStarted(TurnNumber)`
- [ ] `TurnEnded(TurnNumber)`
- [ ] `TechnologyResearched(NationId, TechId)`
- [ ] `WarDeclared(NationId attacker, NationId defender)`
- [ ] `TreatyProposed(NationId from, NationId to, TreatyType)`
- [ ] `TreatyAccepted / TreatyRejected`
- [ ] `ProvinceConquered(ProvinceId, NationId newOwner)`
- [ ] `UnitCreated / UnitDestroyed / UnitUpgraded`
- [ ] `TradeCompleted(NationId buyer, NationId seller, ...)`
- [ ] `BuildingConstructed / BuildingUpgraded`
- [ ] `NationEliminatedFromCouncil`
- [ ] `VictoryAchieved(NationId winner, VictoryType)`

### Verification Strategy
- [ ] **Unit tests**: `cargo test -p domain` — all coordinate, value object, entity, and event tests pass
- [ ] **Coverage check**: Generate coverage report for Domain crate → verify ≥ 90% line coverage
- [ ] **Immutability test**: Attempt to mutate value objects → verify compile-time or runtime errors
- [ ] **Identity test**: Two entities with same ID are equal; two with different IDs are not
- [ ] **Event test**: Publish each domain event type → verify correct handlers fire
- [ ] **Compile check**: Domain crate builds with zero external crate dependencies (verified by inspecting `Cargo.toml`)
