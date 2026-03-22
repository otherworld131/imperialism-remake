# C7. AI declares war and makes peace with same nation in same turn

**Severity:** Critical

**Files:** `crates/domain/src/ai/basic.rs` → `ai_declare_wars()`, `ai_manage_diplomacy()`

**Symptoms:** History log shows:
```
Deneb declared war on Bruhr
Deneb made peace with Bruhr
```
Both events in the same turn, repeated every turn.

**Root cause:** The AI turn sequence calls `ai_declare_wars()` and `ai_manage_diplomacy()`
in the same turn. One function declares war, the other makes peace (or vice versa). There's no
coordination between them, so they contradict each other.

**Fix:**
- `ai_manage_diplomacy()` should not propose peace with nations that were just attacked this turn
- Or: `ai_declare_wars()` should not declare war on nations the AI just made peace with
- Or: Track wars declared this turn and exclude them from peace considerations

- [ ] Prevent war+peace on same nation within same turn
