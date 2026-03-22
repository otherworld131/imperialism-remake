# C2. Workers starve to 0 permanently — no recovery mechanism

**Severity:** Critical — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `resolve_immigration()`

**Root cause:** Immigration requires CannedFood + Clothing + Furniture. With 0 workers,
no production ever occurs, so these goods never exist.

**Fix:** Emergency recruitment: if workforce is 0, grant 1 free worker per turn.

- [x] Add 0-worker emergency in `resolve_immigration()`
- [x] Add regression test (`emergency_recruitment_when_zero_workers`)
