# M10. AI signs pacts/builds embassies with conquered minor nations (0 provinces)

**Severity:** Moderate — FIXED

**Symptoms:** AI signs non-aggression pacts and builds embassies/consulates with minor
nations that have 0 provinces (fully conquered). These diplomatic actions are wasteful
and produce history spam.

**Root cause:** The consulate building, embassy building, pact proposal, and grant
sending functions all iterated over ALL minor nations without checking province count.

**Fix:**
- [x] `ai_build_consulates`: filter to minors with provinces
- [x] Pact proposal: filter to minors with provinces
- [x] Grant sending: filter to minors with provinces
- [x] Embassy building (Phase 4): filter to minors with provinces
- Diplomatic actions now only target minor nations that still exist as independent states
