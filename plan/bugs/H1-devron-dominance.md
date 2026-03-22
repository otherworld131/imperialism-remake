# H1. Devron always dominates — extreme AI imbalance

**Severity:** High

**Cause:** Devron (Aggressive, war every 15 turns) declares war first (lowest interval),
conquers the most territory. Other AIs rarely get a chance because anti-dogpile logic
prevents targeting already-attacked minors, but Devron takes all the spoils because
only the attacking army conquers.

**Fix:** Multiple improvements: other AIs should compete for territory, GP wars should
happen, and the anti-dogpile logic should not let one AI monopolize all targets.

- [ ] Reduce Aggressive war interval from 15 to 20
- [ ] Allow competing AIs to also conquer provinces in wars they joined

**Status:** Significantly improved via C5 (GP wars) and H2 (alliance limits). Devron no longer
monopolizes — Kem and Patagon compete effectively. But not fully solved; more tuning needed.
