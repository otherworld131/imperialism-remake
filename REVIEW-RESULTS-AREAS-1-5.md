# Adversarial Review Results — Areas 1-5

**Total findings: 1 critical, 15 major, 28 minor, 10 suggestions across ~19,000 lines of code.**
**Fixed: 1 critical, 10 major, 10 minor. Deferred: 5 major, 18 minor, 10 suggestions.**

---

## Area 1 — Turn Processor (IMPLEMENTED)

**Planning**: 1 round (Codex reviewer score 4/10 → addressed blockers → 8/10).
**Implementation**: 1 round (score 5/10 → fixed → all passing).
**Files**: `processor.rs`, `scoring.rs`

### Fixes Applied

| ID | Severity | Fix |
|----|----------|-----|
| C1 | critical | Blockade now computed BEFORE trade; `compute_blockade_capacity()` passes adjusted capacity to `resolve_trade_session()` |
| C2 | critical | Added `tech_score`, `treasury_score`, `building_score` fields to `NationScore` struct |
| M1 | major | Extracted `award_first_colony_clippers()` helper, replacing 2 duplicate 30-line blocks |
| F-003 | major | Added zero-guard for `provinces_per_immigrant` division |
| F-010 | major | Blockade computation now filters out anarchic/eliminated nations |
| m1 | minor | Fixed clothing factory labor tracking (`remaining_labor -= ...`) |
| F-008 | minor | Removed 2 dead `let _` no-op expressions |

### Tests Added (4)
- `blockade_reduces_effective_cargo_capacity`
- `blockade_excludes_anarchic_nations`
- `immigration_no_panic_with_zero_provinces_per_immigrant`
- `score_includes_tech_treasury_building_components`

### Deferred
- Nation elimination lifecycle, structured tech events, connectivity downgrade, transport source tracking

---

## Area 2 — Game State & Core Types (REVIEW ONLY)

**Files**: `game_state.rs`, `types.rs`, `nation.rs`, `events.rs`

| # | Severity | Issue | Location |
|---|----------|-------|----------|
| 1 | major | `TurnNumber(0)` bypasses invariant, causes year/quarter underflow | types.rs:25 |
| 2 | major | `Money` arithmetic can overflow/wrap silently | types.rs:131-162 |
| 3 | major | Event structs use `String` instead of typed enums | events.rs:96-137 |
| 4 | minor ✅ | `PlayerId` dead code (removed) | types.rs:15 |
| 5 | minor | `max_improvement_level` all arms return 3 | types.rs:217 |
| 6 | minor ✅ | `is_food` includes Horses (removed) | types.rs:209 |
| 7 | minor | `DomainEvent` lacks Serialize/Deserialize | events.rs:46 |
| 8 | minor ✅ | `ProvinceConquered` missing `old_owner` (added) | events.rs:89 |
| 9 | minor ✅ | `Nation` missing Debug/Clone derives (added) | nation.rs:43 |
| 10 | minor | `GameState` missing Debug derive | game_state.rs:13 |
| 11 | minor | No `remove_province` on Nation | nation.rs |
| 12 | minor | `is_in_anarchy` method shadows field | nation.rs:313 |

---

## Area 3 — Military & Combat (REVIEW ONLY)

**Files**: `combat.rs`, `units.rs`, `ships.rs`, `naval.rs`

| # | Severity | Issue | Location |
|---|----------|-------|----------|
| 1 | **critical** ✅ | **Damage truncation to zero — battles stall when per-unit damage < 5** | combat.rs:306 + units.rs:437 |
| 2 | major ✅ | `take_damage` discards damage < 5, retreat damage is a no-op | combat.rs:386 + units.rs:437 |
| 3 | major ✅ | `unwrap()` on `partial_cmp` panics if NaN | combat.rs:293 |
| 4 | major ✅ | Naval damage distribution inflates/truncates total damage | naval.rs:177 |
| 5 | major ✅ | `effective_firepower()` ignores unit health — no attrition | units.rs:428 |
| 6 | major ✅ | Damage-dealt tracking desyncs after sorting — wrong medal awards | combat.rs:270 |
| 7 | minor | Militia garrison bonus bypasses terrain/fort multipliers | combat.rs:283 |
| 8 | minor ✅ | `general_bonus` picks first General, ignores better ones | combat.rs:86 |
| 9 | minor | `GARRISON_ID_COUNTER` potential ID collision | combat.rs:42 |
| 10 | minor | `fort_defense_bonus` returns 0 for levels > 3 silently | combat.rs:32 |
| 11 | minor | `heal` integer division gives odd medal jumps | units.rs:444 |
| 12 | minor | `prerequisite_tech` vs `required_tech()` return different values | units.rs:54,345 |
| 13 | minor | O(n²) removal in combat loops | combat.rs:352 |

---

## Area 4 — Economy & Production (REVIEW ONLY)

**Files**: `production.rs`, `trade.rs`, `transport.rs`, `buildings.rs`, `civilians.rs`, `labor.rs`

| # | Severity | Issue | Location |
|---|----------|-------|----------|
| 1 | major | Transport: wasted capacity not redistributed when capped | transport.rs:98 |
| 2 | major | Transport: unallocated resources silently dropped | transport.rs:101 |
| 3 | major ✅ | Building `start_expansion` overwrites in-progress expansion | buildings.rs:45 |
| 4 | major | Civilian `CIVILIAN_ID_COUNTER` not reset on save/load | civilians.rs:9 |
| 5 | major ✅ | `available_for_production()` returns wrong metric (removed) | labor.rs:94 |
| 6 | minor | Trade subsidy > price silently clamped to $1 | trade.rs:66 |
| 7 | minor | Trade bid priority uses max score across all sellers | trade.rs:159 |
| 8 | minor | Transport allocation accepts values > 100 | transport.rs:40 |
| 9 | minor ✅ | Miners can't improve Gold/Gems despite max_level=3 | civilians.rs:63 |
| 10 | minor | Production outputs include zero-quantity entries | production.rs:58 |
| 11 | minor ✅ | `start_work(0)` deadlocks civilian permanently | civilians.rs:132 |

**Note**: Production chain math is solid — ratios, zero edges, and consumption accounting all correct.

---

## Area 5 — Map, Terrain & Hex Math (REVIEW ONLY)

**Files**: `coord.rs`, `hex_map.rs`, `tile.rs`, `infrastructure.rs`, `generator.rs`, `province.rs`

| # | Severity | Issue | Location |
|---|----------|-------|----------|
| 1 | major ✅ | Potential infinite loop in `place_nation_centers` fallback | generator.rs:537 |
| 2 | major ✅ | Fallback province uses `HexCoord(0,0)` which is always Sea | generator.rs:198 |
| 3 | major | Port connectivity has no sea-path validation | infrastructure.rs:142 |
| 4 | major | `subdivide_into_provinces` doesn't guarantee contiguity | generator.rs:671 |
| 5 | minor ✅ | Fisher-Yates shuffle off-by-one (out-of-bounds) | generator.rs:829 |
| 6 | minor | `has_fort` / `fort_level` can be inconsistent | tile.rs:18 |
| 7 | minor ✅ | `cluster_food_terrain` overwrites existing resources | generator.rs:913 |
| 8 | minor ✅ | HexMap serialization order non-deterministic | hex_map.rs:26 |
| 9 | minor | `Province::is_coastal` always returns false (stub) | province.rs:103 |
| 10 | minor | hex_lerp epsilon bias | coord.rs:167 |

**Note**: Hex coordinate math is correct — axial system, distance, ring/range, pixel conversion all follow the Red Blob Games reference.

---

## Top Priority Fixes (Recommended Order)

| Priority | Area | Issue | Impact |
|----------|------|-------|--------|
| **P0** | 3 | Combat damage truncation to zero | **Battles can stall entirely** |
| **P1** | 3 | `effective_firepower` ignores health | No attrition dynamics |
| **P2** | 3 | Damage-dealt tracking desync | Wrong medal awards |
| **P3** | 5 | Infinite loop in `place_nation_centers` | Generator can hang |
| **P4** | 5 | Fisher-Yates off-by-one | Out-of-bounds panic |
| **P5** | 4 | Transport wasted capacity | Resources silently lost |
| **P6** | 4 | Building expansion overwrite | Silent resource leak |
| **P7** | 4 | `start_work(0)` deadlocks civilian | Permanent unit loss |
| **P8** | 2 | `TurnNumber(0)` underflow | Year/quarter corruption |
| **P9** | 5 | Port connectivity no sea-path check | False connections |
