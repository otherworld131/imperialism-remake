# H8. Human player economy completely frozen when idle

**Severity:** High

**Symptoms:** After 400 turns of auto-play (no player commands), the human player has:
- Unchanged $10,000 treasury (no income or expenses)
- 0 technologies researched
- 0 army units
- 0 freight cars
- 0 mills/factories
- 1-4 workers (only from emergency recruitment)

Resources DO accumulate in the warehouse (Timber: 305, Coal: 102, etc.) showing that
resource collection from connected tiles works, but nothing is done with them.

**Root cause:** The human player has no AI to manage their economy. Unlike AI nations who
get `run_ai_turns()` called, the human nation sits idle. But even passively, some things
should change:
- Maintenance costs should deduct from treasury (but human has 0 army, so $0 maintenance)
- Resource collection works (verified via warehouse contents)
- No mills = no production = no goods = no immigration beyond emergency

This is partially by design (human needs to issue commands), but the combination of M5
(treasury frozen) and the fact that the human can accumulate 300+ Timber with no way to
use it passively is a UX issue.

**Related:** M5 (human treasury frozen when idle)

- [ ] Consider auto-research of free techs (cost $0) for human player
- [ ] Consider showing a "you need to build mills!" prompt after N turns of idle economy
