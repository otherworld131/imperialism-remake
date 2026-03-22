# H16. Workers starve immediately on Normal difficulty — no food buffer

**Severity:** High — FIXED

**Symptoms:** On Normal difficulty, 4 starting workers consume 4 CannedFood/turn but
the food chain only produces ~1-2 CannedFood/turn (limited by FoodProcessing capacity
and connected province food resources). Workers starve within 2-3 turns, collapsing
the economy before the player can react.

**Root cause:** No starting CannedFood in warehouse. Workers eat immediately but food
processing hasn't bootstrapped yet.

**Fix:**
- [x] Easy/Introductory: 20 starting CannedFood
- [x] Normal: 10 starting CannedFood (enough for ~5-10 turns)
- [x] Hard/NighOnImpossible: 5 starting CannedFood (minimal buffer)

**Results:** Workers survive early game. AI Labor scores reach 530-800 (was 10-400).
AI Transport reaches 635-840. Players now have time to build food infrastructure
before starvation hits.
