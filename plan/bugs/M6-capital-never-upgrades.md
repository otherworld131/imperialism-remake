# M6. Capital province never upgrades from Hamlet

**Severity:** Moderate — FIXED

**Symptoms:** AI's capital province stays at Hamlet while all conquered provinces
become Town. The capital should be the FIRST to upgrade.

**Root cause:** The `update_province_connectivity` function connects non-capital
provinces to the capital, but the capital itself may not have `connected_to_capital`
set. The settlement upgrade logic requires `connected_to_capital == true` to trigger.

- [x] Removed incorrect `is_capital { continue }` skip in `update_settlements`
- [x] Capital now upgrades Hamlet → Village → Town like other provinces
