# M1. FP displays as "-0.0"

**Severity:** Moderate

**File:** `src/main.rs` — overview display

**Fix:** Use `if fp == 0.0 { 0.0 } else { fp }` to normalize negative zero.

- [x] Fix display format
  - **Verified:** All 5 FP displays in post-fix game show "FP: 0.0" (was "FP: -0.0").
