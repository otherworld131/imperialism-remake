# H5. AI freight car building broken

**Severity:** High

**File:** `crates/domain/src/ai/basic.rs` → `ai_build_transport()`

**Root cause:** Returns immediately if `freight_cars > 0`, preventing growth.

**Fix:** Remove the early return. Scale target with province count.

- [x] Rewrite to scale target cars with `province_count.max(5)`
- [x] Update test (`ai_scales_freight_cars_with_provinces`)
  - **Verified:** Test confirms AI builds >1 freight car when given materials. In full gameplay,
    Transport scores still 0 because the upstream material chain (mills → lumber/steel) doesn't
    produce enough yet. The fix is correct but depends on C3 infrastructure connecting first.
