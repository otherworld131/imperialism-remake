# C9. Unlimited army buildup — AI military score hits 29,000+

**Severity:** Critical — FIXED

**File:** `crates/domain/src/ai/basic.rs` → `ai_build_military()`

**Root cause:** Tier 3 army building had no army size cap. Any AI with treasury > $6,000
would build one unit per turn indefinitely. Additionally, tier 3 didn't check affordability
before subtracting cost, risking negative treasury.

**Fix:**
- [x] Added tier3_max caps per personality (Aggressive: 15, Balanced: 12, Economic: 10, Diplomatic: 8)
- [x] Added checked_sub for tier 3 unit cost
- Military scores now range 10-140 instead of 29,000+
