# C12. Devron always wins due to first-mover advantage and double Aggressive

**Severity:** Critical — FIXED

**Root cause:** Two compounding issues:
1. Devron (index 1) was Aggressive personality AND processed first in AI loop
2. Two Aggressive AIs (indices 1 and 3) created imbalanced warfare

**Fix:**
- [x] Changed Devron from Aggressive to Balanced (only Kem remains Aggressive)
- [x] Shuffled AI processing order each turn using deterministic turn-based seed
- Devron no longer dominates. Different nations win different games.
