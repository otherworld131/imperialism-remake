# 21 — Save / Load & Serialization

## Overview

The game must support saving and loading full game state at any point during a turn.
The save format should be stable, versioned, and forward-compatible.

## Checklist

### Save System Architecture
- [x] `SaveRepository` trait — domain defines what needs saving; infrastructure handles how
- [x] `GameStateSnapshot` — serializable representation of entire game state
- [ ] Snapshot includes: map, all nations, all units, all buildings, all diplomatic relations, tech state, turn number, treasury, warehouse, transport orders, pending orders
- [x] Snapshot is a pure data object — no behavior, no references to runtime services
- [x] Unit tests: snapshot creation from live game state

### Serialization Format
- [x] JSON primary format (human-readable, debuggable, moddable)
- [ ] Binary format option for faster load times (`bincode`, `postcard`, or `rmp-serde`)
- [x] Schema versioning: each save file includes a format version number
- [x] Forward compatibility: newer game versions can load older saves
- [x] Migration system: version N save → version N+1 transformation
- [ ] Compression: saves compressed with gzip/zstd for disk efficiency
- [x] Unit tests: serialize → deserialize roundtrip produces identical state
- [x] Unit tests: schema migration from older versions

### Save Slots
- [x] Multiple save slots (at least 20)
- [x] Autosave every N turns (configurable, default: every turn)
- [x] Quicksave / quickload hotkeys
- [x] Save file naming: descriptive (nation name + turn number + timestamp)
- [x] Save file location: platform-appropriate user data directory
- [ ] Unit tests: save slot management

### Save File Contents
- [x] Game metadata: version, timestamp, player name, nation, difficulty, turn number
- [ ] Map state: all tile data (terrain, resources, improvements, infrastructure)
- [ ] Nation state: treasury, diplomatic standing, tech research status, building levels
- [ ] Unit state: all civilian, army, and naval units with positions, health, medals
- [ ] Diplomatic state: all relations, treaties, consulates, embassies
- [ ] Economic state: warehouse contents, production assignments, transport allocations
- [ ] Trade state: active subsidies, trade history
- [ ] AI state: persistent AI memory/goals per nation
- [ ] Pending orders: orders submitted for current turn but not yet resolved
- [ ] RNG state: random number generator state for deterministic replay
- [ ] Unit tests: every state component serializes and deserializes correctly

### Load System
- [x] Load from save file → reconstruct full `GameState`
- [x] Validate loaded state: all invariants hold (no corrupt data)
- [x] Graceful error handling: corrupted save → informative error message, not crash
- [ ] Loading screen with progress indication for large saves
- [ ] Unit tests: load validation catches corrupted data

### Save Browser UI
- [x] List all saves with metadata (date, nation, turn, difficulty)
- [x] Sort by date, nation, or turn
- [x] Preview: show minimap and key stats for selected save
- [x] Delete save option (with confirmation)
- [ ] Export/import saves for sharing

### Verification Strategy
- [x] **Unit tests**: Run test suite — all persistence tests pass
- [x] **Roundtrip test**: Create game state → save → load → compare → identical
- [ ] **Corruption test**: Tamper with save file bytes → load → verify graceful error, no crash
- [ ] **Migration test**: Create save with version N schema → migrate to N+1 → load successfully
- [ ] **Autosave test**: Play 5 turns → verify 5 autosave files created
- [ ] **Performance test**: Save a full game state (7 nations, 120 provinces, all units) → save < 1 second, load < 2 seconds
- [x] **Cross-version test**: Save with build X, load with build X+1 → verify compatibility
