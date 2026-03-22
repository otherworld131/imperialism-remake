# H7. Alliance obligation joins spam — 15+ per turn

**Severity:** High

**File:** `crates/domain/src/turn/processor.rs` → `resolve_alliance_obligations()`

**Symptoms:** A single nation (e.g., Zimm) gets 15+ "joined war against X (alliance obligation)"
events in a single turn, one for every minor nation its ally is at war with.

**Root cause:** When a GP forms an alliance with another GP, the alliance obligation code
fires for ALL wars the ally is engaged in, including wars against already-conquered minor
nations (those with 0 provinces). Each obligation join is logged as a separate event.

**Fix:**
- Skip alliance obligations for wars against minor nations with 0 provinces
- Or: Only fire alliance obligations for active wars (where target still has provinces)
- Consider: Only join wars that started this turn, not pre-existing wars

- [ ] Filter out wars against already-defeated nations in alliance obligation resolution
