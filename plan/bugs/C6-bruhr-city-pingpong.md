# C6. Bruhr City ping-pong — same province conquered back and forth every turn

**Severity:** Critical

**Files:** `crates/domain/src/ai/basic.rs` → `ai_declare_wars()`, `ai_military_strategy()`
`crates/domain/src/turn/processor.rs` → `resolve_combat()`

**Symptoms:** Game history shows 3+ Great Powers conquering the same city from each other
every single turn for 100+ turns:
```
Deneb conquered Bruhr City from Patagon
Devron conquered Bruhr City from Deneb
Patagon conquered Bruhr City from Devron
```

**Root cause:** Multiple AIs declare war on the same minor nation and attack its capital.
Combat resolves sequentially within a single turn — attacker A takes the city from the minor,
then attacker B takes it from A, then attacker C takes it from B. Next turn, the cycle repeats
because all three are still at war with each other's provinces.

The province flip-flops ownership within a single turn processing, which is non-physical —
a province should only change hands once per turn.

**Fix options:**
- Only allow one province ownership change per turn (first attacker wins)
- Track already-contested provinces and skip subsequent attacks on the same province
- Make peace after conquering all of a minor's provinces

- [ ] Prevent multiple ownership changes for same province in one turn
- [ ] AI should make peace when target minor has 0 remaining provinces
