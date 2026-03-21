# 15 — Turn Engine & Game Loop

## Overview

Each turn represents one quarter (3 months). The game starts in 1815 Q1 and can run until
1915. Players interact with 5 sequential screens per turn, then all orders are resolved
simultaneously.

## Checklist

### Turn Structure
- [x] `TurnNumber` → year + quarter mapping (Turn 1 = 1815 Q1, Turn 400 = 1914 Q4, Turn 401 = 1915 Q1)
- [ ] Each turn, all players interact with 5 screens in order
- [ ] After all players submit, orders are resolved
- [ ] Resolution order: production → transport → trade → diplomacy → military → combat → scoring
- [x] Unit tests: turn-to-year conversion, resolution ordering
### Five Sequential Screens (Player Phase)
1. **Map Screen**
   - [ ] Deploy civilian specialists to terrain tiles (prospectors, miners, farmers, foresters, engineers, ranchers, drillers)
   - [ ] Issue military orders: move units, attack, establish beachheads
   - [ ] View map state: terrain, improvements, infrastructure, units, provinces
   - [ ] Build infrastructure via engineers: railroads, depots, ports, forts
   - [ ] Ctrl+click zoom icon to reveal map key
2. **Transport Screen**
   - [ ] Allocate freight car capacity across resource types
   - [ ] View production vs. transport capacity
   - [ ] Build new freight cars in Railyard
3. **Industry Screen**
   - [ ] Assign resources to mills and factories for processing
   - [ ] Recruit and train workers (Capitol, Trade School, University)
   - [ ] Build/expand processing buildings
   - [ ] View labor pool and assignments
4. **Trade Screen**
   - [ ] Set trade offers (sell goods, set prices)
   - [ ] Set trade bids (buy resources, set prices)
   - [ ] View Minor Nation trade partners and available goods
   - [ ] Manage trade subsidies
5. **Diplomacy Screen**
   - [ ] View world map and nation information
   - [ ] Build Trade Consulates and Embassies
   - [ ] Propose treaties (pacts, alliances, peace)
   - [ ] Declare war
   - [ ] Offer cash grants and subsidies
   - [ ] View Council of Governors status
- [ ] Unit tests: screen data correctly calculated for each phase

### Turn Resolution Pipeline
- [x] `TurnProcessor` trait orchestrates the entire resolution
- [ ] Steps executed in deterministic order:
  1. [x] **Civilian actions resolve** — specialists improve tiles, engineers build infrastructure
  2. [x] **Production resolves** — mills/factories process resources using assigned inputs
  3. [x] **Transport resolves** — freight cars deliver resources to capital
  4. [x] **Immigration resolves** — new workers arrive if canned food + clothing + furniture available
  5. [x] **Technology resolves** — newly purchased techs take effect; new techs become available
  6. [x] **Trade resolves** — offers/bids matched, transactions executed, revenue generated
  7. [x] **Diplomacy resolves** — treaties accepted/rejected, relationship scores updated
  8. [x] **Military movement resolves** — units move to ordered destinations
  9. [x] **Combat resolves** — battles fought in provinces with opposing forces
  10. [x] **Naval combat resolves** — naval battles in contested sea zones
  11. [x] **Conquest resolves** — provinces change ownership, rewards granted
  12. [x] **Maintenance resolves** — military maintenance costs deducted
  13. [x] **Scoring resolves** — game score recalculated
  14. [x] **Victory check** — Council of Governors vote if decade boundary
  15. [x] **Newspaper generated** — events of the turn compiled
  16. [x] **New turn begins** — turn counter advances
- [ ] Unit tests: each resolution step in isolation
- [ ] Integration test: full turn resolution with all systems active

### Newspaper
- [x] Generated after turn resolution, displayed before next turn begins
- [x] "Imperial Times" — dated to the quarter
- [x] Reports: new technology discoveries, military actions, diplomatic events
- [ ] Some items have no gameplay impact (flavor text, historical references)
- [ ] Some items report events before advisor notifications
- [ ] Unit tests: newspaper event collection and formatting

### Game State Management
- [x] `GameState` aggregate root holds entire game state
- [ ] State transitions are deterministic given the same inputs (critical for multiplayer sync)
- [ ] State snapshots for undo/redo consideration
- [x] State serialization for save/load (see plan 21)
- [ ] Unit tests: state determinism — same inputs → same outputs

### Player Turn Submission
- [ ] Single player: player submits, AI players generate orders instantly, resolution runs
- [ ] Multiplayer: all players must submit before resolution (see plan 22)
- [ ] "End Turn" button triggers submission
- [ ] Orders validated before submission (reject invalid orders)
- [ ] Unit tests: order validation rules

### AI Turn Orders
- [x] AI generates orders for all 5 screens (map, transport, industry, trade, diplomacy)
- [x] AI orders generated via `AiDecisionMaker` trait (see plan 16)
- [x] AI orders pass through same validation as human orders
- [ ] AI processing time should be fast (< 2 seconds for all AI players)
- [x] Unit tests: AI orders pass validation

### Verification Strategy
- [x] **Unit tests**: Run test suite — all turn engine tests pass
- [ ] **Integration test**: Full turn resolution with 7 nations (1 human + 6 AI) → no errors, valid state transition
- [ ] **Determinism test**: Run same turn with same inputs 100 times → identical results every time
- [ ] **Performance test**: Measure turn resolution time with full 7-player game → must complete within 5 seconds
- [ ] **10-turn simulation**: Run 10 turns fully automated → verify game progresses sensibly (no crashes, no invalid states)
- [ ] **100-turn simulation**: Run 100 turns → verify no state corruption, memory leaks, or runaway values
- [ ] **End-game test**: Run until 1915 → verify game ends correctly with final scoring
