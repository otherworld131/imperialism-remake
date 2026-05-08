# Lua → Rust drift audit (2026-05-08)

> **Historical snapshot.** Captured before the Lua-bake refactor on 2026-05-08
> (Trello card #477). After that change WASM reads the same Lua values native
> does, so the drift listed below is no longer observable. Preserved as a
> record of what the WASM build was silently doing pre-fix.
>
> Note: the per-call-site `unwrap_or(<literal>)` defaults in `lua_bridge.rs`
> still differ from the Lua values — they're now dead-code mirrors that fire
> only if a Lua key goes missing. Resolving those literal mismatches is a
> separate hygiene task; it does not affect runtime behavior while every
> Lua key is present.

## Method

Walked every `table.get("KEY").unwrap_or(DEFAULT)` site in
`crates/domain/src/ai/lua_bridge.rs` and compared the DEFAULT against the
value in the corresponding `scripts/*.lua` file. Drift = the WASM build
silently used DEFAULT, but native (CLI/batch) used the Lua value.

## game_config (scripts/config/game.lua)

| Key | Lua value | Rust fallback (WASM) | lua_bridge.rs line |
|---|---|---|---|
| `ai_embassy_min_relation` | 25 | 50 | 68 |
| `voluntary_incorporation_threshold` | 95 | 90 | 70–71 |
| `trade_relation_turn_interval` | 1 | 3 | 74 |
| `civilian_target_tiles_per_worker` | 8 | 3 | 133–135 |
| `rest_heal_amount` | 35 | 10 | 249 |

All other game.lua keys (~80 of them) matched their `unwrap_or` defaults.

## balanced personality (scripts/ai/balanced.lua)

| Key | Lua | Rust fallback | Result |
|---|---|---|---|
| `trade_priority` | 0.5 | 0.5 | match |
| `alliance_preference` | 0.5 | 0.5 | match |
| `min_army_size` | 3 | 3 | match |
| `max_army_size` | 7 | 7 | match |
| `infrastructure_budget` | 2000 | 2000 | match |
| `research_strategy` | "cheapest" | "cheapest" | match |
| `worker_threshold` | 5 | 5 | match |

0 drift rows for balanced. (The `unwrap_or` defaults matched balanced
exactly, which meant WASM silently used balanced behavior for ALL
personalities.)

## aggressive personality (scripts/ai/aggressive.lua)

| Key | Lua | Rust fallback | lua_bridge.rs line |
|---|---|---|---|
| `trade_priority` | 0.3 | 0.5 | 1411 |
| `alliance_preference` | 0.2 | 0.5 | 1412 |
| `min_army_size` | 5 | 3 | 1413 |
| `max_army_size` | 12 | 7 | 1414 |
| `infrastructure_budget` | 1500 | 2000 | 1415 |
| `research_strategy` | "military" | "cheapest" | 1416 |
| `worker_threshold` | 3 | 5 | 1419 |

## diplomatic personality (scripts/ai/diplomatic.lua)

| Key | Lua | Rust fallback | lua_bridge.rs line |
|---|---|---|---|
| `trade_priority` | 0.8 | 0.5 | 1411 |
| `alliance_preference` | 0.9 | 0.5 | 1412 |
| `min_army_size` | 2 | 3 | 1413 |
| `max_army_size` | 4 | 7 | 1414 |
| `infrastructure_budget` | 2500 | 2000 | 1415 |
| `research_strategy` | "economic" | "cheapest" | 1416 |
| `worker_threshold` | 4 | 5 | 1419 |

## economic personality (scripts/ai/economic.lua)

| Key | Lua | Rust fallback | lua_bridge.rs line |
|---|---|---|---|
| `trade_priority` | 0.7 | 0.5 | 1411 |
| `alliance_preference` | 0.6 | 0.5 | 1412 |
| `max_army_size` | 6 | 7 | 1414 |
| `infrastructure_budget` | 3000 | 2000 | 1415 |
| `research_strategy` | "expensive" | "cheapest" | 1416 |
| `worker_threshold` | 3 | 5 | 1419 |

## units / ships — 0 drift rows

Both use strict `require!`/`require_ship!` macros that fail-or-succeed
atomically. The `lua_baseline_unit_stats_match` test enforces that
units.lua == Rust baseline.

## Summary

- **Total drift rows: 25**
- Per section: game_config=5, balanced=0, aggressive=7, diplomatic=7,
  economic=6, units=0, ships=0
- **Most-impactful drifts:**
  - All non-balanced personalities collapsed to balanced behavior on WASM
    for the 7 core fields (army size, trade priority, alliance preference,
    infrastructure budget, research strategy, worker threshold).
  - `civilian_target_tiles_per_worker` 8→3 ≈ 2–3× more civilian hires.
  - `rest_heal_amount` 35→10 ≈ 3.5× slower healing.
  - `trade_relation_turn_interval` 1→3 ≈ 3× slower trade-diplomacy gain.
