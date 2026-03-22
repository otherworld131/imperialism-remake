# H9. Human auto-play doesn't build factories or freight cars

**Severity:** High — FIXED

**Root cause:** `auto_manage_human()` only built mills and researched free techs.
No logic for factories or freight cars.

**Fix:**
- [x] Auto-build factories when corresponding mill exists (costs 1 Lumber + 1 Steel each)
- [x] Auto-build up to 2 freight cars per turn (costs 1 Lumber + 1 Steel each)
- Human now reaches 2+ factories and 1+ freight car during auto-play
