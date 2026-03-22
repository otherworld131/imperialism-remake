# C10. Province swap loop — two nations trade provinces every turn

**Severity:** Critical — FIXED

**File:** `crates/domain/src/turn/processor.rs` → `resolve_combat()`

**Root cause:** Two nations at war could each conquer one province from the other
per turn (different provinces, so `already_contested` didn't block). This created
an endless tit-for-tat swap pattern filling the history log.

**Fix:**
- [x] Added `already_conquered` tracking — each nation can only conquer one province per turn
- Province swap spam eliminated from late-game history
