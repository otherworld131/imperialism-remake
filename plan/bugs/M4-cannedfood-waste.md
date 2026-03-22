# M4. CannedFood stockpiles uselessly with 0 workers

**Severity:** Moderate — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `process_food()`

**Fix:** Skip food processing when there are no workers.

- [x] Add worker count check
