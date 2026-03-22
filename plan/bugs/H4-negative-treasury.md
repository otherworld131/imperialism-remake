# H4. Negative treasury allowed

**Severity:** High — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `apply_maintenance()`

**Root cause:** Maintenance costs, trade subsidies, and trade transactions could push
treasury below zero with no protection.

**Fix:** Changed `BANKRUPTCY_FLOOR` from `-$5,000` to `$0`. Treasury is now capped at
zero after all spending operations.

- [x] Changed BANKRUPTCY_FLOOR to Money::ZERO
- [x] Updated bankruptcy tests
- [x] Added regression test (`treasury_floor_zero_after_maintenance`)
