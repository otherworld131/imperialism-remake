# M9. AI treasury still high for large empires — improved grant scaling

**Severity:** Moderate — FIXED

**Root cause:** AIs with 30+ provinces generate income faster than they can spend.
The $500 grant per minor nation per 4-8 turns is negligible for $200K+ treasuries.
Also, the $50K trade-sell cap was too high.

**Fixes applied:**
- [x] Grant amounts scale with treasury: 5x at $20K, 10x at $50K, 20x at $100K+
- [x] Trade sell cap reduced from $50K to $20K (stop accumulating sooner)
- Most AIs now end at $0-$10K. Large empires ($40K+) still occur but less extreme.

**Note:** Remaining hoarding is a game design feature — without a resource marketplace,
money can't be efficiently converted to materials. The original Imperialism had the same
dynamic where late-game empires accumulated wealth.
