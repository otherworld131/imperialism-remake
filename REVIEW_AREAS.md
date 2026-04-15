# Adversarial Review Areas

Each area is scoped to a coherent unit that fits comfortably in one review session.
Use with: `/adversarial-review`

---

## Area 1 — Turn Processor (the monster)

**~7,800 lines — the single most critical file in the codebase.**

```
crates/domain/src/turn/processor.rs
crates/domain/src/turn/scoring.rs
crates/domain/src/turn/mod.rs
```

Focus: correctness of the full turn resolution pipeline, ordering dependencies between
phases, off-by-one errors, resource conservation invariants, edge cases when nations are
eliminated mid-turn.

---

## Area 2 — Game State & Core Types

**~2,700 lines — the data backbone everything else reads and mutates.**

```
crates/domain/src/game_state.rs
crates/domain/src/types.rs
crates/domain/src/nation.rs
crates/domain/src/events.rs
```

Focus: data integrity, impossible states that the type system should prevent but doesn't,
missing or inconsistent derives (Clone/Serialize), event ordering guarantees.

---

## Area 3 — Military & Combat

**~3,800 lines — battle resolution, unit stats, naval operations.**

```
crates/domain/src/military/combat.rs
crates/domain/src/military/units.rs
crates/domain/src/military/ships.rs
crates/domain/src/military/naval.rs
crates/domain/src/military/mod.rs
```

Focus: combat balance (damage formulas, morale), edge cases (zero-strength units, empty
fleets), naval invasion rules, unit stacking limits, dead-unit cleanup.

---

## Area 4 — Economy & Production

**~3,000 lines — resources, production chains, trade, transport, labor.**

```
crates/domain/src/economy/production.rs
crates/domain/src/economy/trade.rs
crates/domain/src/economy/transport.rs
crates/domain/src/economy/buildings.rs
crates/domain/src/economy/civilians.rs
crates/domain/src/economy/labor.rs
crates/domain/src/economy/mod.rs
```

Focus: resource accounting (no duplication/leaks), production chain correctness,
transport capacity limits, trade deal fairness calculations, civilian unit lifecycle.

---

## Area 5 — Map, Terrain & Hex Math

**~4,200 lines — spatial foundation for everything.**

```
crates/domain/src/hex/coord.rs
crates/domain/src/hex/mod.rs
crates/domain/src/map/hex_map.rs
crates/domain/src/map/tile.rs
crates/domain/src/map/infrastructure.rs
crates/domain/src/map/generator.rs
crates/domain/src/map/province.rs
crates/domain/src/map/mod.rs
```

Focus: hex coordinate math correctness (especially wrapping/edge), neighbor lookups,
distance calculations, map generation fairness, province connectivity, infrastructure
placement rules.

---

## Area 6 — AI System

**~9,300 lines — the largest module, all AI decision-making.**

```
crates/domain/src/ai/mod.rs
crates/domain/src/ai/assessment.rs
crates/domain/src/ai/common.rs
crates/domain/src/ai/diplomacy.rs
crates/domain/src/ai/economy.rs
crates/domain/src/ai/labor.rs
crates/domain/src/ai/military.rs
crates/domain/src/ai/naval.rs
crates/domain/src/ai/research.rs
crates/domain/src/ai/spending.rs
crates/domain/src/ai/tactical.rs
crates/domain/src/ai/lua_bridge.rs
```

Focus: decision quality (does the AI make sane choices?), panics on unexpected game states,
Lua bridge safety (sandboxing, error handling), budget allocation logic, strategic vs
tactical coherence. **Split into two sessions if needed: AI-core (mod, assessment, common,
spending, labor, research) and AI-strategy (military, naval, economy, diplomacy, tactical,
lua_bridge).**

---

## Area 7 — Diplomacy & Tech

**~1,600 lines — smaller but rules-dense systems.**

```
crates/domain/src/diplomacy/relations.rs
crates/domain/src/diplomacy/mod.rs
crates/domain/src/tech/tree.rs
crates/domain/src/tech/mod.rs
```

Focus: treaty validation (mutual exclusion, prerequisite checks), relation score clamping,
tech prerequisite graph (cycles? unreachable nodes?), effect application correctness.

---

## Area 8 — Data Loading & Scenarios

**~1,500 lines — how game definitions and scenarios get loaded.**

```
crates/domain/src/data/mod.rs
crates/domain/src/data/loader.rs
crates/domain/src/data/definitions.rs
crates/domain/src/scenarios.rs
crates/domain/src/scripting/mod.rs
crates/domain/src/scripting/game_api.rs
crates/domain/src/scripting/sandbox.rs
```

Focus: error handling on malformed data files, Lua sandbox escape vectors, missing
field defaults, scenario validation (e.g. nation count matches map).

---

## Area 9 — WASM Bridge & Web Frontend Bridge

**~1,500 lines — the FFI boundary between Rust and the browser.**

```
crates/wasm-bridge/src/lib.rs
```

Focus: serialization correctness (Rust <-> JS), panic safety (unwinding across FFI),
missing error propagation, API surface bloat, state management across calls.

---

## Area 10 — Web Frontend (TypeScript/React)

**~2,600 lines — the player-facing UI.**

```
web/src/App.tsx
web/src/components/HexMap.tsx
web/src/wasm.ts
web/src/components/UnitPanel.tsx
web/src/components/GameSetup.tsx
web/src/components/NavalPanel.tsx
web/src/components/CivilianPanel.tsx
```

Focus: state synchronization with WASM, rendering performance (HexMap is 900 lines),
user input handling, accessibility, error boundaries, memory leaks from canvas/WebGL.

---

## Area 11 — Application & Infrastructure Layers

**~1,300 lines — queries, persistence, data loading.**

```
crates/application/src/queries.rs
crates/application/src/lib.rs
crates/infrastructure/src/persistence.rs
crates/infrastructure/src/data_loader.rs
crates/infrastructure/src/lib.rs
```

Focus: query completeness (does the app layer expose everything the frontend needs?),
save/load round-trip fidelity, versioning of save formats, error handling on corrupt saves.

---

## Area 12 — CLI Entry Point & Batch Runner

**~5,400 lines — the main binary.**

```
src/main.rs
src/gui.rs
```

Focus: argument parsing, batch simulation correctness, output format, error reporting,
graceful shutdown, separation of concerns (main.rs is very large — should logic move
to library crates?).

---

## Area 13 — Integration Tests

**~3,200 lines — test quality and coverage.**

```
tests/properties.rs
tests/simulation.rs
tests/military.rs
tests/architecture.rs
tests/benchmarks.rs
tests/edge_cases.rs
tests/diplomacy.rs
tests/test_helpers.rs
```

Focus: are tests actually testing the right invariants? Flaky tests? Missing coverage
for critical paths? Architecture tests enforcing the dependency rules?

---

## Area 14 — Game Data Definitions

**RON/JSON config files — the data that drives gameplay.**

```
data/definitions/nations.ron
data/definitions/units.ron
data/definitions/ships.ron
data/definitions/buildings.ron
data/definitions/terrain.ron
data/definitions/technologies.ron
data/definitions/production.ron
data/definitions/difficulty.ron
data/localization/en.json
```

Focus: balance, completeness (any referenced IDs that don't exist?), consistency between
related files (e.g. units reference techs that exist), typos in string IDs.

---

## Area 15 — Lua Scripts

**~720 lines — AI behavior profiles and game config.**

```
scripts/ai/diplomatic.lua
scripts/ai/aggressive.lua
scripts/ai/economic.lua
scripts/ai/balanced.lua
scripts/config/game.lua
scripts/tech/tech_effects.lua
```

Focus: Lua-Rust contract correctness (do scripts return what Rust expects?), balance
between AI personalities, missing nil checks, config values that contradict RON data.

---

## Suggested Review Order

| Priority | Area | Why |
|----------|------|-----|
| 1 | Area 1 (Turn Processor) | Highest risk — orchestrates everything |
| 2 | Area 3 (Military & Combat) | Player-visible correctness |
| 3 | Area 6 (AI System) | Largest module, most complex logic |
| 4 | Area 4 (Economy) | Resource accounting bugs are subtle |
| 5 | Area 2 (Game State) | Data integrity affects all systems |
| 6 | Area 9 (WASM Bridge) | FFI boundary = bug magnet |
| 7 | Area 5 (Map & Hex) | Spatial math errors propagate widely |
| 8 | Area 12 (CLI/Main) | Unusually large — may need refactoring |
| 9 | Area 10 (Web Frontend) | User-facing quality |
| 10 | Area 7-8, 11, 13-15 | Lower risk, smaller scope |
