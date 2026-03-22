# M4. CannedFood stockpiles uselessly with 0 workers

**Severity:** Moderate

**File:** `crates/domain/src/turn/processor.rs` → `process_food()`

**Fix:** Should not process food when there are no workers.

- [x] Add worker count check
  - **Verified:** With emergency recruitment (C2), nations always have >= 1 worker,
    so this check prevents the edge case of wasteful food processing on 0-worker nations.
