# H2. Ordune allies ALL Great Powers on turn 1

**Severity:** High — FIXED

**File:** `crates/domain/src/ai/basic.rs` → `ai_manage_diplomacy()`

**Root cause:** Diplomatic AI proposes alliances with all GPs at game start.

**Fix:** Cap at 2 GP alliances max, only propose after turn 10.

- [x] Add max alliance cap (2) with in-loop tracking
- [x] Add minimum turn requirement (turn >= 10)
- [x] Add regression tests (`no_alliances_formed_before_turn_10`, `alliances_capped_at_two`)
