# C4. Phantom defender units in combat

**Severity:** Critical — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `resolve_combat()`

**Root cause:** `create_garrison()` always generates Militia regardless of actual army.

**Fix:** Only create garrison for capital provinces. Non-capital undefended provinces
are auto-conquered without battle.

- [x] Modify garrison creation to check for actual army or capital status
- [x] Add regression test (`undefended_non_capital_province_is_auto_conquered`)
