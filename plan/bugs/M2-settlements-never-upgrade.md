# M2. Settlements never upgrade from Hamlet

**Severity:** Moderate

**File:** `crates/domain/src/turn/processor.rs` → `update_settlements()`

**Likely cause:** Upgrade conditions are never met (population too low, no connected provinces).

- [ ] Investigate and reduce thresholds or fix prerequisites
