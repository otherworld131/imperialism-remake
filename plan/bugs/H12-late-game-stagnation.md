# H12. Late-game stagnation — no activity after minor nations conquered

**Severity:** High — FIXED

**Root cause:** Two compounding issues:
1. Scoring formula only measured static quantities (army, workers, freight, provinces)
   that all hit hard caps by turn ~100. Economic growth was invisible to scores.
2. GP-vs-GP wars rarely fired and when they did, the AI never attacked.

**Fixes applied:**
- [x] Scoring now includes technology (30 pts/tech), treasury (up to 500), buildings (10 pts each)
- [x] Scores now grow throughout the game as AIs research tech and accumulate wealth
- [x] GP war attacks use actual garrison counts instead of phantom estimates
- [x] GP attack threshold relaxed (2/3 defender ratio instead of strict superiority)
- [x] GP province targeting prioritized over minor nation targets
- [x] GP wars target weakest province instead of capital

**Results:** Scores at turns 100/200/400 now show continuous growth.
Late-game tech research extends to turn 300+.
