# H11. AI hoards massive treasury — $70K-$265K unspent

**Severity:** High — FIXED

**Symptoms:** AI nations accumulate $70,000-$265,000 by end of game while having
only 10-12 army units and not researching late-game techs. Money sits idle.

**Root cause:** AI has no late-game spending logic beyond:
- Tech research (caps at whatever the AI personality prefers)
- Mill/factory expansion (caps at available resources)
- Army building (caps at 8-15 units)
- Infrastructure (built early, no ongoing expansion)

The AI should spend surplus treasury on: more tech research, building expansion,
additional merchant ships, or diplomatic grants to minor nations.

- [x] AI research fallback: if treasury > $10K and normal pick unaffordable, try cheapest available
- [x] AI builds 2 units/turn when treasury > $20K (within army cap)
- [x] AI expands mills/factories when treasury > $15K (invest in capacity)
