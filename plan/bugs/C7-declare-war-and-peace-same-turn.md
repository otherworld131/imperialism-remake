# C7. AI declares war and makes peace with same nation in same turn

**Severity:** Critical — FIXED

**Root cause:** `ai_manage_diplomacy` made peace with 0-province nations, then
`ai_declare_wars` and `ai_military_strategy` re-declared war on them.

**Fixes applied:**
- [x] `ai_manage_diplomacy` skips peace if pending attacks exist against that nation
- [x] `ai_declare_wars` filters out fully conquered minors (0 provinces)
- [x] `ai_military_strategy` filters out fully conquered minors
- [x] Alliance obligations skip wars against defeated nations

**Results:** No more war+peace on same nation in same turn.
