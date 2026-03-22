# C1. "Bruhr voluntarily joined" spam — fires every turn forever

**Severity:** Critical — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `resolve_voluntary_incorporations()`

**Root cause:** No check for already-incorporated minor nations (0 provinces).

**Fix:** Skip minor nations with 0 provinces.

- [x] Add `province_ids.is_empty()` guard at top of loop
- [x] Add regression test (`no_reincorporation_of_already_incorporated_minor`)
