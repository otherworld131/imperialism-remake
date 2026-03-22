# C3. AI never builds railroads, depots, or ports

**Severity:** Critical — FIXED

**File:** `crates/domain/src/ai/basic.rs` → `ai_build_infrastructure()` only builds mills/factories

**Root cause:** No code exists for AI to build railroads or depots. Provinces stay disconnected.

**Fix:** Added `ai_build_map_infrastructure()` that builds depots on capital and adjacent provinces,
then railroads to connect them.

- [x] Add `ai_build_map_infrastructure()` function
- [x] Call it from `run_ai_turns()`
- [x] Add regression test (`ai_builds_depot_on_capital`)
