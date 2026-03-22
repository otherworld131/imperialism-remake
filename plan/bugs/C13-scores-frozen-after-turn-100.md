# C13. Game scores completely frozen after turn ~100

**Severity:** Critical — FIXED

**Root cause:** Scoring formula in `scoring.rs` only measured army size, workers,
freight cars, and province count — all of which hit hard caps early. Economic growth
(treasury, technology, buildings) was invisible to scores.

**Fix:**
- [x] Added technology score: 30 points per researched tech
- [x] Added treasury score: up to 500 points (treasury/100, capped)
- [x] Added building score: 10 points per building

Scores now grow continuously from turn 1 through 400.
