# C8. Nation conquers province from itself

**Severity:** Critical

**Symptoms:** History shows "Haxaco conquered Bruhr City from Haxaco" — a nation
conquering a province it already owns.

**Root cause:** After a GP conquers a minor nation's last province, the province now
belongs to the GP. But the pending attack was set up against the minor's province. On
the next turn, if another GP attacks the same province (now owned by the first GP), the
ownership transfer works correctly. However, if the SAME GP has a pending attack queued
(perhaps from alliance obligations or a stale attack order), it "conquers" from itself.

**Fix:**
- In `resolve_combat()`, skip attacks where the attacker already owns the target province
- Clean up stale pending_attacks before processing combat

- [ ] Add guard in resolve_combat: skip if attacker already owns province
