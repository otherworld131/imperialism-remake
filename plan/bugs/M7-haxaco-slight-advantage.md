# M7. Haxaco (Economic) wins slightly more often than other AIs

**Severity:** Moderate — ACKNOWLEDGED

**Observation:** Haxaco wins 5-6 out of 8 games when human plays as Deneb (nation 0).
This is because Economic personality invests in expensive techs (high tech score)
and accumulates provinces efficiently.

**Mitigation applied:**
- [x] Rebalanced personalities: 3 Balanced, 2 Aggressive, 1 Economic, 1 Diplomatic
- [x] Reduced province score weight from 100 to 75 per province
- [x] Other games are won by Patagon, Zimm, Ordune — variety exists

**Status:** Acceptable for current gameplay. Full balance would require deeper
personality tuning of war thresholds, expansion rates, and scoring weights.
The original Imperialism also had nations that tended to perform better due
to map position, which this game simulates.
