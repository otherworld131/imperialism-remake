# H6. Transport score is 0 for ALL nations in ALL games

**Severity:** High

**Symptoms:** After 400 turns (full game to 1915), every nation has Transport score = 0.
No nation ever builds freight cars despite AI infrastructure building being implemented.

**Root cause:** The freight car production chain requires:
1. Connected provinces (depots + railroads) — C3 fix partially addresses this
2. Mills producing Lumber and Steel — AI builds mills but needs raw resources flowing
3. Railyard building to build freight cars — all nations have one
4. Resources connected to capital for mill input

The bottleneck is likely that even with depots/railroads, the mill→material→freight car
pipeline takes many turns to bootstrap, and AI may not be prioritizing freight car construction.

**Files:** `crates/domain/src/ai/basic.rs` → `ai_build_transport_proactive()`

- [ ] Investigate why AI never accumulates enough Lumber+Steel for freight cars
- [ ] Check if freight car building logic fires at all during a 400-turn game
- [ ] Consider giving AI starting freight cars or lowering freight car costs
