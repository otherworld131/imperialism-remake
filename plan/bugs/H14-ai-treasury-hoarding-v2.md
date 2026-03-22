# H14. AI treasury hoarding v2 — $100K-$200K unspent after late-game tech depletion

**Severity:** High — FIXED

**Root cause:** After all techs are researched (~1890), AI had nothing left to spend on.
Army capped, infrastructure built, basic fleet built. Cash piled up from ongoing trade.

**Fixes applied:**
- [x] Merchant ship cap increases to 5 when treasury > $5K
- [x] Warship cap increases when treasury > $8K
- [x] Worker recruitment cap increases from 2x to 3x provinces when treasury > $20K
- [x] AI stops selling resources when treasury > $50K (keeps materials for ships/units)

**Results:** AI treasury reduced from $186K to $52K (Haxaco) and $3K-$8K (others).
Labor scores reach 400-550. Transport reaches 745. Much healthier economy.
