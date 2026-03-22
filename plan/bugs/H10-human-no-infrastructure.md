# H10. Human auto-play doesn't build infrastructure or sell resources

**Severity:** High — FIXED

**Root cause:** `auto_manage_human()` was missing infrastructure building, resource
selling, and affordable tech research. Provinces stayed disconnected, treasury frozen,
and only free techs were researched.

**Fix:**
- [x] Auto-build depots and railroads on all provinces (capital + non-capital)
- [x] Auto-sell excess resources for income (same as AI trade logic)
- [x] Auto-research cheapest affordable tech (not just $0 techs)
- Human provinces now connect, treasury fluctuates, 6-8 techs researched
