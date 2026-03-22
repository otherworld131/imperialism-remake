# M3. Resources accumulate infinitely

**Severity:** Moderate — FIXED

**Fix:** Added `apply_warehouse_caps()` function that runs each turn after production:
- Raw resources capped at 50 per Warehouse capacity level
- Materials capped at 50 per Warehouse capacity level
- Finished goods capped at 25 per Warehouse capacity level
- Excess is silently discarded (spoilage/waste)
- Expanding the Warehouse building increases all caps

- [x] Added warehouse capacity system with per-resource caps
- [x] 5 regression tests for cap behavior
