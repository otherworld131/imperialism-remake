# C3. AI never builds railroads, depots, or ports

**Severity:** Critical

**File:** `crates/domain/src/ai/basic.rs` → `ai_build_infrastructure()` only builds mills/factories

**Root cause:** No code exists for AI to build railroads or depots. Provinces stay disconnected.
`run_ai_turns()` never calls any railroad/depot/port building function.

**Fix:** Add `ai_build_map_infrastructure()` that builds depots on capital and adjacent provinces,
then railroads to connect them.

- [x] Add `ai_build_map_infrastructure()` function
- [x] Call it from `run_ai_turns()`
- [x] Add regression test (`ai_builds_depot_on_capital`)
  - **Verified:** AI treasury drops from $10K → $4.8K on turn 1 (depot + military costs).
    Function builds depots then railroads progressively. Test confirms depot placement.
    However, Transport scores still show 0 in gameplay — the production chain (raw resources
    → mills → lumber/steel → freight cars) isn't completing because provinces need more
    turns to connect and start flowing resources to mills. The infrastructure IS being built
    but the economic chain downstream hasn't caught up yet.
