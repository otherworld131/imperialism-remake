# H12. Late-game stagnation — no activity after minor nations conquered

**Severity:** High — PARTIALLY FIXED

**Symptoms:** After all 16 minor nations are conquered (~turn 80-100), history consists
only of tech research. No GP wars, no territorial changes, no economic growth.
Last 200 turns are uneventful.

**Root cause:** GP-vs-GP war requires `remaining_minors <= 2 && turn > 40` AND the
attacker must have 5+ provinces more than the target. With only the Aggressive AI
(Kem) considering GP wars, and all other AIs being Balanced/Economic/Diplomatic,
GP wars rarely fire.

- [x] Economic AI now considers GP wars (50-turn interval)
- [x] Province advantage threshold lowered from +4 to +2
- [x] Minimum army for GP wars lowered from 5 to 4
- [ ] Add economic events (trade agreements, treaties) to fill late-game history
