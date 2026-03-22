# H6. Transport score is 0 for ALL nations in ALL games

**Severity:** High — FIXED

**Root cause:** Chicken-and-egg deadlock. Mills require Lumber+Steel to build, but Lumber+Steel
can only be produced by mills. AI could never bootstrap its industrial economy.

Additionally, `ai_military_strategy` had a separate war declaration path that targeted
fully-conquered minor nations (0 provinces), causing pointless wars.

**Fixes applied:**
- [x] First mill of each type is now free (no material cost) — breaks the bootstrap deadlock
- [x] All difficulties now give starting materials (Lumber/Steel/Fabric)
- [x] AI `ai_military_strategy` now filters out 0-province minor nations
- [x] Attack targets use actual province ownership instead of stale capital references

**Results:** Transport scores now reach 3700+ in full games. Labor scores reach 1000+.
AI nations build mills, factories, and freight cars successfully.
