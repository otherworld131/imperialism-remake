# H15. AI diplomacy invisible in history and embassy building restricted

**Severity:** High — FIXED

**Root cause:** Two issues:
1. Diplomatic events (pacts, alliances) were only in `actions` vec but never
   recorded in `game.history`, making them invisible in the history log.
2. Only the Diplomatic personality built embassies. Other AIs stuck at
   consulate level, preventing pact signing.

**Fixes applied:**
- [x] All AI personalities now build embassies (with treasury threshold by personality)
- [x] Pact and alliance events now recorded in game.history with deduplication
- Diplomatic activity now visible in history: pacts appearing from turn 238 onward
