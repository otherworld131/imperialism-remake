# M5. Human player treasury frozen when idle

**Severity:** Moderate — RESOLVED (by design)

**Cause:** Human player has $0 maintenance (no army) and no active spending.
Treasury stays at starting value because there are no expenses without player commands.

This is by design — the human player must issue commands to spend money.
Auto-play now provides basic automation (mills, free techs) via H8 fix.

- [x] Investigated — working as designed, mitigated by H8 auto-management
