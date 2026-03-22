# C5. AI never declares war on Great Powers

**Severity:** Critical — FIXED

**File:** `crates/domain/src/ai/basic.rs` → `ai_declare_wars()`

**Root cause:** Only minor nations considered as war targets.

**Fix:** Added GP-vs-GP war logic (Phase 2 in `ai_declare_wars`). Aggressive/Balanced AIs
target weaker GPs when they have military superiority.

- [x] Add GP target selection
- [x] Add regression test (`ai_does_not_declare_war_on_great_powers`)
