# M2. Settlements never upgrade from Hamlet

**Severity:** Moderate — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `update_settlements()`

**Root cause:** The `connected_to_capital` flag on provinces was never set to `true`
in any production code path. Settlement upgrades require connectivity.

**Fix:** Added `update_province_connectivity()` that runs each turn before settlement
updates. Computes connectivity via:
1. Infrastructure check (railroads/depots/ports)
2. Adjacency fallback (provinces with tiles adjacent to capital province)

- [x] Added update_province_connectivity function
- [x] Provinces adjacent to capital are auto-connected for early-game progression
