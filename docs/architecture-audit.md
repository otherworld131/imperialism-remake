# Architecture Audit — Imperialism Remake

**Date:** 2026-04-24
**Scope:** Backend crates (`domain`, `application`, `infrastructure`, `wasm-bridge`) and their adherence to the hexagonal / clean-architecture rules stated in `CLAUDE.md`.
**Out of scope:** UI / presentation / frontend code quality.

---

## 1. Executive Summary

The backend **broadly follows the forward dependency direction** mandated by hex architecture: no domain → application or domain → infrastructure imports were detected, and the Lua sandbox is tight. However, several **critical strays** have accumulated at the seams:

| Area | Status | Notes |
|------|--------|-------|
| Forward dependency direction | 🟢 Green | No reverse deps between layers |
| Lua sandbox | 🟢 Green | `os`, `io`, `loadfile`, `dofile`, `require`, `debug`, `load` all nilled |
| Application view models exist | 🟢 Green | But bypassed by WASM bridge |
| Frontend/backend seam | 🔴 Red | WASM bridge serializes raw `GameState` to JSON — no typed command/query API |
| Domain purity (no infra leaks) | 🔴 Red | `serde`/`ron` are direct deps of domain crate; serde derives scattered across domain types |
| Domain purity (Lua/Rust split) | 🔴 Red | AI tier thresholds, coalition weights, combat modifiers, economic score formulas are magic numbers in Rust |
| Clean code / complexity | 🔴 Red | `turn/processor.rs` is 14,672 lines; `resolve_combat()` is 962 lines; `GameState` has 104 fields |
| Determinism | 🔴 Red | `HashMap` iteration inside `apply_warehouse_caps()` is the likely root cause of the known "Devron" treasury divergence |
| Backward-compat shims | 🟡 Yellow | `BattleConfig::legacy()` still live; "legacy behavior" comments in `ai/economy.rs` |
| Error handling | 🔴 Red | 836+ `.unwrap()` / `.expect()` calls in backend; mid-turn panic risk |

**Overall grade: yellow-trending-red.** The skeleton is correct; the flesh has grown in the wrong places. The single highest-leverage fix is **splitting `turn/processor.rs`** — it is simultaneously the god-module, the home of the non-determinism bug, and the source of the worst god-functions.

---

## 2. Layer Boundaries & Dependencies

### 🔴 Critical

**L-1. WASM bridge exposes raw `GameState` as JSON**
- `crates/wasm-bridge/src/lib.rs:42-56, 138-193`
- Functions like `wasm_new_game` return the entire `GameState` serialized to JSON; `wasm_set_human_player` deserializes, mutates, and re-serializes raw domain state.
- **Violates:** "Frontend receives view-model structs — never raw domain entities" (CLAUDE.md, architecture diagram).
- **Why it matters:** Any change to internal domain fields silently changes the wire protocol. Application-layer view models (`MapScreenData`, `TradeScreenData`, `DiplomacyScreenData` in `application/queries.rs:9-55`) exist but are bypassed.

**L-2. WASM bridge has no typed command/query interface**
- `crates/wasm-bridge/src/lib.rs` (entire file)
- All calls are stringly-typed: `fn wasm_process_turns(game_json: &str, n: usize) -> String`.
- **Violates:** "Calls into Application layer via typed commands & queries" (CLAUDE.md).
- **Why it matters:** No validation layer, no clean seam. Frontend acts as an orchestrator of domain state.

**L-3. Serde/ron are domain crate dependencies**
- `crates/domain/Cargo.toml:7-8` declares `serde` and `ron` as direct (non-optional) dependencies.
- **Violates:** "Domain crate depends only on `std` + `mlua`" (CLAUDE.md).
- **Why it matters:** Infrastructure concern (serialization) has become a first-class citizen in the domain.

**L-4. Serde derives embedded throughout domain types**
- `crates/domain/src/game_state.rs:28-35` — `PoliticalSnapshot`, `GameState`
- `crates/domain/src/types.rs:4, 21-24` — `NationId`, `TurnNumber`, all ID types
- `crates/domain/src/nation.rs` — `Nation` with 19+ `#[serde(default)]` attributes
- **Violates:** Domain types' public shape is locked to serde's contract.
- **Why it matters:** The save/wire format can no longer be changed without a domain refactor. Makes #L-1 difficult to fix at the root.

### 🟡 Warning

**L-5. Presentation imports domain directly, bypassing application layer**
- `crates/presentation/src/app.rs:2-3` — `use domain::game_state::new_game; use domain::types::Difficulty;`
- `crates/presentation/src/hex_renderer.rs:2-4` — `use domain::{game_state::GameState, hex::HexCoord, types::*};`
- **Violates:** Presentation should consume view models via application queries.

**L-6. CLI binary imports domain and infrastructure directly**
- `Cargo.toml:43-44`, `src/main.rs:11-13`
- **Violates:** CLI is a presentation layer and should call application commands/queries, not `process_turn` directly.

### 🟢 Green

- **No reverse dependencies.** Domain imports neither application nor infrastructure. Application imports only domain. Infrastructure imports application + domain.
- **Lua sandbox integrity** (`crates/domain/src/scripting/sandbox.rs:15-28`) — `os`, `io`, `loadfile`, `dofile`, `require`, `debug`, `load` all nilled. Comprehensive sandbox tests.

---

## 3. Domain Purity & Lua/Rust Split

CLAUDE.md rule: *"Rust holds the engine; Lua holds the numbers. If you find a magic number in Rust that controls game feel or AI choices, move it to Lua."*

### 🔴 Critical

**D-1. AI military tier thresholds hardcoded per personality**
- `crates/domain/src/ai/military.rs:85-125`
- Treasury tiers ($1500/$3000/$6000 for Aggressive, $3000/$8000/$15000 for Diplomatic, …) and unit caps (4/7/15) hardcoded in match blocks.
- Unit build costs ($1000 Grenadiers, $500 Regulars, $2000 LightArtillery).
- **Fix:** Move to `scripts/ai/tuning.lua` (or similar); make Lua the primary source, no Rust fallback.

**D-2. Coalition strength and economic score weights hardcoded**
- `crates/domain/src/ai/assessment.rs:70-80`
- `mil_weight: 0.5`, `prov_weight: 0.3`, `econ_weight: 0.2`, `momentum_weight: 0.15`, `naval_weight: 0.3`, `sigmoid_steepness: 3.0`.
- `crates/domain/src/ai/assessment.rs:112-114` — `treasury / 10_000.0 + buildings * 0.1 + workers * 0.05`.
- **Fix:** Move to Lua AI config. These drive war/peace decisions.

**D-3. `game.history` string parsing for correctness-critical AI logic** *(= Trello Card #77)*
- `crates/domain/src/ai/assessment.rs:199-210` — `compute_momentum()` greps `"conquered"` + nation names
- `crates/domain/src/ai/assessment.rs:223-229` — `find_war_start_turn()` greps `"declared war"` / `"joined war"`
- `crates/domain/src/ai/assessment.rs:466-477` — `evaluate_war_worthiness()` greps `"conquered"`
- `crates/domain/src/ai/military.rs:474-480` — `ai_declare_wars()` greps `"declared war on"`
- **Fix:** Introduce `enum HistoryEvent { ProvinceConquered { winner, loser, … }, WarDeclared { by, against }, … }`; store `Vec<(TurnNumber, HistoryEvent)>` instead of `Vec<(TurnNumber, String)>`.

### 🟡 Warning

**D-4. Treaty decision thresholds as scattered floats**
- `crates/domain/src/ai/assessment.rs:426-451` — 10+ magic floats (`0.55`, `-0.2`, `0.35`, …) for peace/alliance/war decisions across personality branches.

**D-5. Combat balance constants in Rust**
- `crates/domain/src/military/combat.rs:37-85`
- Terrain defense (Mountain 0.50, Hills 0.30, Forest 0.20, Swamp 0.15)
- Fort bonuses (L1 0.20, L2 0.40, L3 0.60)
- General medal bonus (`1.10 + medals * 0.05`)
- Retreat damage (`health * 0.10`), siege reduction (`base * 0.5`)

**D-6. AI civilian hiring thresholds hardcoded**
- `crates/domain/src/ai/labor.rs:28-150` — Miner > $1000, Prospector/Farmer > $2000, worker-placement doubling at $20k.

**D-7. Diplomatic constants hardcoded**
- `crates/domain/src/ai/diplomacy.rs:12-40` — Consulate cost $500, grant amounts/intervals per personality.

**D-8. Tech tree hardcoded in Rust**
- `crates/domain/src/tech/tree.rs:48-150+`
- `TechTree::new()` is a giant match statement with 28+ `Technology` structs.
- `TechTree::from_technologies()` exists as an alternative path but isn't used in production.
- **Fix:** Move to `data/definitions/technologies.ron` or `scripts/tech/tree.lua`; load at startup.

**D-9. `TechEffect` variants use `String` where enums already exist**
- `crates/domain/src/tech/tree.rs:17-33`
- `UnlockUnit(String)`, `UnlockBuilding(String)`, `UpgradeUnit { from: String, to: String }`, `EnableCivilian(String)`, `EnableInfrastructure(String)`.
- **But** `BuildingType`, `ArmyUnitType` enums are already defined in the codebase.
- **Fix:** Replace string fields with their corresponding typed enums.

**D-10. Backward-compat shims still live**
- `crates/domain/src/military/combat.rs:48-59` — `BattleConfig::legacy()` method with "Legacy behavior: only the attacker retreats, 60% FP loss" comment. Still invoked at `combat.rs:311`.
- `crates/domain/src/ai/economy.rs:1357` — "legacy behavior" comment for warehouse expansion costs.
- **Violates:** CLAUDE.md "No backward compatibility" rule.
- **Fix:** Remove (or, if intentional house-rule/difficulty mode, move to config and rename).

---

## 4. Code Quality & Hotspots

### 🔴 Critical

**C-1. `turn/processor.rs` is 14,672 lines**
- Single file orchestrating economy, military, diplomacy, civilians, settlements, immigration, scoring, and newspaper generation.
- 30+ functions of mixed concerns.
- **Fix:** Split into per-phase modules: `turn/economy_phase.rs`, `turn/military_phase.rs`, `turn/diplomacy_phase.rs`, `turn/civilian_phase.rs`, `turn/scoring_phase.rs`, `turn/news_phase.rs`.

**C-2. `GameState` is a 1,534-line god-object with 104 fields**
- `crates/domain/src/game_state.rs`
- Mixes: game loop state (turn, difficulty), world state (provinces, nations, hex_map), archives (newspaper, battles, political snapshots), transient event state (events, pending_attacks, pending_moves), and a Lua engine reference.
- **Fix:** Split into `WorldState` + `GameArchive` + `TransientState` (+ `ScriptingContext` for the Lua handle).

**C-3. God-functions in `processor.rs`**
| File:Line | Function | Lines |
|-----------|----------|-------|
| `processor.rs:2635` | `resolve_combat()` | 962 |
| `processor.rs:4934` | `resolve_diplomatic_proposals()` | 338 |
| `processor.rs:2042` | `resolve_trade_session()` | 292 |
| `processor.rs:5707` | `resolve_rewards()` | 216 |
| `processor.rs:1238` | `resolve_civilian_actions()` | 224 |

- `resolve_combat()` hits 7-level nesting at `processor.rs:2661-2880`: `for → if (contested) → if (conquered) → if (lost_province) → if (adjacency) → if (ownership) → match (relation)`.
- **Fix:** Extract pure functions (`compute_battle_outcome()`, `validate_trade_route()`, `apply_engineer_task()`) that return `Result<Outcome, Error>` without mutating; keep mutation in separate `apply_*` functions. Unblocks unit testing.

**C-4. HashMap iteration → non-determinism** *(known bug, likely root cause)*
- `processor.rs:6121-6137` — `apply_warehouse_caps()` iterates `warehouse.values_mut()`, `materials.values_mut()`, `goods.values_mut()` in non-deterministic order.
- `processor.rs:1241-1254` — `owned_by_nation` HashMap built inside `resolve_civilian_actions()`.
- **Fix:** Replace with `BTreeMap` or `IndexMap`. One-line change for the warehouse case; verifies immediately against `tests/simulation.rs::test_determinism`.

**C-5. 836+ `.unwrap()` / `.expect()` in backend**
- Concentrated in `processor.rs` (lines 2722-2725, 6473, 6487, 6535, 12159, 12172, 12546, 12561, 13655, 14123).
- 2 outright `panic!()` in `ai/economy.rs:2142, 2447` ("expected KeepCommitment").
- **Risk:** Mid-turn panic leaves save file corrupted.
- **Fix:** Audit each; convert game-loop panics to typed errors; keep unwraps only in test fixtures.

### 🟡 Warning

**C-6. Global mutable ID counters**
- `AI_UNIT_ID_COUNTER` (`ai/common.rs`, `ai/military.rs`)
- `CIVILIAN_ID_COUNTER` (`economy/civilians.rs:39`)
- `GARRISON_ID_COUNTER` (`military/combat.rs:93`)
- **Risk:** Breaks future parallel-AI, deterministic replay, and multiplayer sync.
- **Fix:** Move counters into `GameState` (or `TransientState` post-split); mutate in single-threaded turn phase.

**C-7. 40+ copies of `match personality { Aggressive => …, Diplomatic => …, … }`**
- Across `ai/military.rs`, `ai/economy.rs`, `ai/diplomacy.rs`, `ai/spending.rs`, `ai/labor.rs`.
- **Fix:** Introduce a `PersonalityConfig` struct (loaded from Lua) so each AI function reads one config instead of re-dispatching on the enum.

**C-8. Duplicated Lua config read boilerplate**
- `ai/military.rs:42-48`, plus 5 other AI modules.
- Pattern `#[cfg(feature = "lua")] if let Some(v) = lua_cfg.x() { return v; }` repeated.
- **Fix:** Extract helper that takes a closure returning `Option<T>` and a Rust default.

**C-9. SRP violations in AI modules**
| File | Lines | Mixed concerns |
|------|-------|---------------|
| `ai/military.rs` | 2,577 | Army building, unit placement, field tactics, Lua config |
| `ai/economy.rs` | 2,450 | Building construction, civilian hiring, speculation, bank loans, food production |
| `ai/spending.rs` | 1,738 | Military + economic + diplomatic spending all in one scoring loop |
| `ai/diplomacy.rs` | 1,415 | Trade offers + treaty negotiation + pact assessment |

**C-10. Testability: core flows can't be unit-tested**
- `resolve_combat()`, `resolve_civilian_actions()`, `resolve_trade_session()` all require a fully-assembled `GameState` and mutate it inline.
- **Fix:** See C-3 — pure-function extraction.

**C-11. Error-handling inconsistency**
- `Option<T>`, `Result<T, String>`, custom enums, `anyhow`, `panic!`, `unreachable!` all mixed.
- `application/queries.rs` calls `.expect()` on `get_nation()` (6 sites).
- `infrastructure/persistence.rs` uses `Result<(), String>` for all I/O.
- **Fix:** Define `DomainError`, `ApplicationError`, `PersistenceError` enums; remove string errors.

### 🟢 Green / Minor

- **Focused modules**: `military/combat.rs` (1,924 lines), `map/infrastructure.rs` (1,480 lines), `map/generator.rs` (1,467 lines) are large but single-concern and cohesive.
- **Dead code suppressions** via `#[allow(dead_code)]` in `ai/economy.rs:55`, `ai/lua_bridge.rs:341-353` — likely refactoring residue; audit before removing (some may be Lua-exposed).

---

## 5. Prioritized Backlog

The following is a suggested order; "User decides" caveats apply. Numbers in parentheses reference finding IDs above.

### Tier 1 — Cheap, high-leverage, isolated
1. **Fix warehouse-caps non-determinism** (C-4) — `HashMap` → `BTreeMap`/`IndexMap` in `Nation` warehouse/materials/goods. Verifies against `test_determinism`. **~1 PR, <50 LOC.**
2. **Delete `BattleConfig::legacy()` and legacy comment in `ai/economy.rs:1357`** (D-10) — enforces no-backward-compat policy.
3. **Replace `TechEffect` string fields with existing enums** (D-9) — `BuildingType`, `ArmyUnitType` already exist.

### Tier 2 — Structured-events & Lua/Rust split
4. **Card #77: history parsing → structured `HistoryEvent` enum** (D-3). Enables typed AI reasoning and unblocks future replay tooling.
5. **Move AI tier thresholds & coalition weights to Lua** (D-1, D-2) — `scripts/ai/tuning.lua`, Lua-primary (no Rust fallback).
6. **Move tech tree to data file** (D-8) — `data/definitions/technologies.ron` or `scripts/tech/tree.lua`; retire hardcoded `TechTree::new()`.
7. **Move combat balance & civilian hiring thresholds to Lua** (D-5, D-6, D-7).

### Tier 3 — Seams (architectural, high-impact)
8. **WASM bridge: typed commands + application view models** (L-1, L-2). Stop serializing raw `GameState`; use `application/queries.rs` models that already exist.
9. **Extract pure `compute_battle_outcome()` from `resolve_combat()`** (C-3) — unit-testable, sets pattern for C-10.
10. **Split `turn/processor.rs` into per-phase modules** (C-1) — unblocks everything else.
11. **Split `GameState` into `WorldState` / `GameArchive` / `TransientState`** (C-2).
12. **Promote global ID counters into `GameState`** (C-6).

### Tier 4 — Deep architectural
13. **Remove serde derives from domain types** (L-3, L-4). Probably requires a serialization-facade crate (`infrastructure::serialization::DomainSnapshot`) that mirrors domain shapes. Large PR, but the right endgame.
14. **Wrap CLI and presentation in application commands/queries** (L-5, L-6).
15. **Unify error handling with typed error enums** (C-11).

### Tier 5 — Hygiene sweeps
16. **Audit `.unwrap()` / `.expect()` in `processor.rs`** (C-5) — convert mid-turn-panic risks to errors.
17. **Extract `PersonalityConfig` to collapse 40+ match copies** (C-7).
18. **Extract Lua-config-read helper** (C-8).

---

## 6. Suggested Trello Follow-ups

Existing cards:
- **Card #59** — "Check mechanics for refactoring" → this document.
- **Card #77** — "Refactor history string parsing to structured events" → finding D-3.

Candidates for new cards on "UP Next backend":
- Fix non-determinism: `BTreeMap` in warehouse caps (C-4)
- Split `turn/processor.rs` by phase (C-1)
- Split `GameState` into `WorldState` / `GameArchive` / `TransientState` (C-2)
- Extract `compute_battle_outcome()` pure function from `resolve_combat` (C-3)
- WASM bridge: typed commands + view models (L-1, L-2)
- Move AI tuning constants to Lua (D-1, D-2)
- Move tech tree to data/Lua (D-8)
- Replace `TechEffect` string fields with existing enums (D-9)
- Delete `BattleConfig::legacy()` (D-10)
- Move global ID counters into `GameState` (C-6)
- Unify error handling with typed enums (C-11)

---

## 7. Second-Pass Review (2026-04-25)

A second pass spot-checked the headline claims and reconciled this audit with the parallel `economy-lessons-from-rust-imperialism.md` note. Conclusions:

### Verified
- `crates/domain/src/turn/processor.rs` is **14,672 lines** (confirmed).
- `crates/domain/src/game_state.rs` is **1,534 lines** (confirmed).
- `crates/wasm-bridge/src/lib.rs` is **5,662 lines** — even larger than implied by L-1/L-2; the seam problem is bigger than the audit calls out.
- `crates/domain/Cargo.toml` declares `serde` and `ron` as direct (non-optional) deps (confirmed; lines 7–8). The CLAUDE.md "domain depends only on std + mlua" rule is genuinely violated.
- No first-class `Reservation` type exists in the domain. The handful of `reserved` matches in the codebase are scattered field flags (in `processor.rs`, `types.rs`, `military/combat.rs`, `ai/diplomacy.rs`), not a coherent reservation layer. This corroborates the parallel economy-lessons note.

### Severity reframing
- **L-3/L-4 (serde in domain) is directionally right but is realistically Tier 4, not 🔴 Critical.** Removing serde derives from domain types means standing up a parallel "snapshot" mirror crate — a large, mechanical change that won't move any user-visible needle until the WASM bridge (L-1/L-2) is fixed first. The audit's prioritized backlog already places this in Tier 4; the table at the top of section 1 overstates the urgency.
- **L-1/L-2 (raw `GameState` over WASM) and C-1/C-2 (god-objects) are the real critical seam.** These are what make every other refactor expensive, and they're what break the architecture diagram in CLAUDE.md most visibly.
- **C-4 (HashMap → BTreeMap in `apply_warehouse_caps`) remains the single best Tier 1 fix.** It's <50 LOC, directly attacks the known Devron determinism bug, and `tests/simulation.rs::test_determinism` provides immediate verification.

### Cross-cutting with `economy-lessons-from-rust-imperialism.md`
The economy-lessons doc and this audit converge on the same root cause from different angles:
- This audit attacks it structurally: **split `processor.rs` by phase, split `GameState`/`Nation` by concern.**
- The economy-lessons doc attacks it semantically: **introduce reservation/snapshot/phase boundaries inside the economy.**

The highest-leverage move that both docs imply is to **extract a `NationEconomy` substruct first** (audit C-2 + economy-lessons §8). That gives every downstream improvement (reservations, AI snapshots, plan/reserve/execute split, unified inventory API) a natural home to land in. Without that decomposition, the reservation work in particular will tangle further into `processor.rs` and make things worse before they get better.

### Refined recommended order
1. Fix C-4 warehouse-caps determinism (independently valuable, ~1 PR).
2. Delete the audit's other Tier 1 items (D-9, D-10) — pure cleanup.
3. Extract `NationEconomy` substruct from `Nation` (the seam both docs need).
4. Split `turn/processor.rs` by phase (C-1) — into `economy_phase.rs`, `military_phase.rs`, etc.
5. Then layer the economy-lessons §1–§4 work (unified inventory → reservations → AI snapshots → plan/reserve/execute) inside the new structure.
6. Then attack L-1/L-2 (typed WASM commands + view models). Domain-side decomposition makes this dramatically cheaper because there are real component boundaries to project view models from.
7. L-3/L-4 (serde out of domain) last, as the endgame cleanup.

### What this audit understates
- **Test-suite coupling.** Many domain assertions live inside `processor.rs`-anchored tests. Splitting `processor.rs` means rewriting a meaningful slice of the test suite simultaneously. Plan for this; it's not a free refactor.
- **The `wasm-bridge` size.** At 5,662 lines, the bridge has accumulated logic (not just translation). It is itself a god-module that mirrors `processor.rs` on the FFI side. Treat L-1/L-2 as a bridge-decomposition task, not just an API redesign.

### What this audit overstates
- The 🔴 Red on "Domain purity (no infra leaks)" implies serde-in-domain is a bug. It's a deliberate-feeling shortcut whose cost is real but bounded — the wire format coupling is the actual problem, not the import itself.
- The 836+ unwrap count is alarming but most are in cold paths or test fixtures. The audit's instruction to "audit each" is correct; the headline number suggests a worse situation than spot-checks reveal.

### Net assessment
Audit conclusions are accurate and the prioritization is broadly right. The main correction is to **down-prioritize serde-removal and up-prioritize `Nation` decomposition** as the sequencing keystone for everything else. Treat this audit and the economy-lessons doc as two views of the same refactor program, not two separate workstreams.
