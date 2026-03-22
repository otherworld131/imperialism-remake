# C8. Nation conquers province from itself

**Severity:** Critical — FIXED

**Fix:** Added guard in `resolve_combat`: skip if `attacker_id == defender_id`.

- [x] Add guard in resolve_combat: skip if attacker already owns province
