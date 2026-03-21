# 13 — Combat System

## Overview

Land battles can be player-controlled (tactical) or AI-automated. Naval battles are always
AI-resolved. Combat occurs when armies in adjacent provinces are ordered to attack, or when
a province is invaded via beachhead.

## Checklist

### Battle Initiation
- [x] Attacker orders army units to move into an enemy-occupied province
- [x] Defender's garrison (Militia/Minutemen) + stationed army defend
- [x] Counter-attacks: if defender sends reinforcements during the same turn, they arrive for a secondary engagement
- [x] Counter-attack reinforcements "have already moved" — no opportunity fire during first move
- [x] Battle can be player-controlled or auto-resolved (player choice in preferences)
- [x] Unit tests: battle initiation conditions, counter-attack eligibility

### Tactical Battle Map
- [ ] Hex-based battlefield (separate from strategic map)
- [ ] Terrain from the province affects battlefield layout
- [ ] Forts appear on the tactical map as defensive structures
- [ ] Attacker and defender deploy units on opposite sides
- [ ] Auto-deployment available (but suboptimal for counter-attacks)
- [ ] Counter-attack deployment tip: place faster units in front (auto-deploy doesn't do this)
- [ ] Unit tests: battlefield generation from province terrain

### Turn-Based Tactical Combat
- [ ] Units take turns based on initiative (influenced by General medals + force composition)
- [ ] Each unit per turn: move (up to movement points) → fire (if in range)
- [ ] **Opportunity fire**: defending units fire when enemy enters their range during enemy movement
- [x] Firepower calculation: base FP × medal modifier × terrain modifier
- [ ] Range: maximum distance (in hex tiles) a unit can fire
- [ ] Movement: hex tiles a unit can traverse per combat turn
- [x] Damage applied to target health in 5% increments
- [x] Unit destroyed at 0% health
- [ ] Unit tests: initiative ordering, opportunity fire triggers, damage calculation

### Combat Modifiers
- [x] **Medals**: 4 medals ≈ 2× firepower; medal holders take less damage and recover faster
- [x] **Terrain**: defensive bonuses for hills, forests, fortifications
- [x] **Fort defense**: units inside forts receive significant defense bonuses
- [ ] **Fort destruction**: Sappers tunnel to fort walls; heavy artillery bombards from range
- [x] Unit tests: all modifier calculations

### Sapper Mechanics (Tactical)
- [ ] Sappers use half movement to dig one tunnel space
- [ ] Stationary sapper: 2 tunnel spaces per turn
- [ ] Tunnel reaches fort wall → explosive placement → section destroyed
- [ ] "S" key: skip all non-sapper units until next sapper's turn
- [ ] Skip interrupted if any unit takes damage
- [ ] Unit tests: tunnel progress per turn, fort section destruction

### Battle Resolution
- [x] Battle ends when one side is eliminated or retreats
- [x] Retreating units suffer additional damage
- [x] Victorious attacker occupies the province
- [x] Surviving units retain damage and earn medals based on performance
- [x] Province garrison (Militia) is destroyed on conquest — not captured
- [x] Unit tests: victory/defeat determination, retreat mechanics, medal award

### Province Conquest
- [ ] Conquering a Minor Nation capital: new army unit starts with 1 medal + Armory statue
- [ ] Conquering a Great Power capital: Capitol expansion (recruitment ratio 4:1 → 3:1)
- [x] Minor Nation provinces become colonies of the conquering power
- [x] Great Power provinces change ownership
- [x] Garrison in conquered provinces: none until player stations units (garrison_count reset to 0)
- [x] Unit tests: conquest effects and rewards

### Naval Combat (AI-Only)
- [x] Naval battles always resolved automatically
- [x] Combat factors: firepower, range, armor, hull points, speed
- [x] Damage reduces hull points; ship sinks at 0
- [x] Battle report provided to player with outcome summary
- [x] Merchant ships in a blockaded zone may be sunk (some survive based on armor/speed)
- [x] Unit tests: naval combat algorithm, battle report generation

### Auto-Resolve (Land)
- [x] Player can choose to auto-resolve land battles in preferences
- [x] AI applies basic tactical logic (positioning, firing priority)
- [x] Results should be reasonable but slightly worse than optimal player control
- [x] Unit tests: auto-resolve produces valid outcomes

### Verification Strategy
- [x] **Unit tests**: Run test suite — all combat tests pass
- [x] **Deterministic combat test**: Fixed seed → same battle always produces same result
- [ ] **Integration test**: 3 Regulars attack province with 4 Militia → verify battle resolves, correct winner determined
- [ ] **Integration test**: Sapper siege scenario → verify tunnel progress over multiple turns → fort destroyed
- [ ] **Balance test**: Run 1000 battles with various force compositions → verify win rates are reasonable
- [ ] **Naval test**: Fleet of 3 Frigates vs 1 Ship-of-the-Line → verify plausible outcomes over 100 runs
- [x] **Counter-attack test**: Defender sends reinforcements → verify they arrive with no opportunity fire on first move
