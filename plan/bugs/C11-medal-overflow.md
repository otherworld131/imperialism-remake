# C11. Medal accumulation inflates military scores

**Severity:** Critical — FIXED

**Root cause:** Medals had no cap. Units surviving many combats accumulated
dozens of medals, each giving +25% firepower. This inflated military scores
to 4,000+ even with capped army sizes.

**Fix:**
- [x] Capped medals at 4 per unit (2x firepower multiplier, matching original game)
- Military scores now consistently 10-140 across all games
