# L6. `build railroad` / `build depot` give no feedback when they fail

**Severity:** Low / UX — ACKNOWLEDGED

**Symptoms:** `build railroad` and `build depot` silently do nothing when they fail
(e.g., wrong tile, insufficient funds). No error message is shown.

**Note:** These commands work correctly with auto-play infrastructure building.
The interactive versions need better error feedback, but the game is playable
without it since auto-play handles infrastructure automatically.
