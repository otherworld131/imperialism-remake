# H8. Human player economy completely frozen when idle

**Severity:** High — FIXED

**Root cause:** Human player has no AI to manage economy during auto-play.

**Fix:** Added `auto_manage_human()` function called during `auto` play that:
- Auto-researches any available $0-cost tech (free techs)
- Auto-builds first LumberMill, SteelMill, TextileMill for free (matching AI bootstrap)

Human player now gets 3 mills and 2 techs during auto-play. Materials are produced,
warehouse fills up to capacity caps.

- [x] Auto-research free techs during auto-play
- [x] Auto-build first mills during auto-play
