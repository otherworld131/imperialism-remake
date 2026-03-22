# C6. Bruhr City ping-pong — same province conquered back and forth every turn

**Severity:** Critical — FIXED

**Fixes applied:**
- [x] Province can only change hands once per turn (`already_contested` set)
- [x] AI makes peace with nations that have 0 provinces
- [x] `ai_declare_wars` filters out fully conquered minors (actual province ownership check)
- [x] `ai_military_strategy` filters out fully conquered minors
- [x] Attack targets use actual province ownership, not stale capital references
- [x] Self-conquest prevented (attacker == defender skip)

**Results:** 0 Bruhr war declarations in full 400-turn games.
