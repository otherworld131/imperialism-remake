# 09 — Diplomacy & Treaties

## Overview

Diplomacy is identified as "key to winning the game." Players manage relationships with both
Minor Nations (trade partners / conquest targets) and other Great Powers (allies / rivals).
The Council of Governors voting system is the primary victory path.

## Checklist

### Diplomatic Relationship Model
- [x] `DiplomaticRelation` entity — pair of NationIds + relationship score + treaty history
- [x] Relationship score: numeric value tracking friendliness (-100 to +100 or similar)
- [ ] Score modifiers: trade frequency, grants, subsidies, broken promises, wars
- [ ] Track number of distinct commodity types traded per turn (not quantity — types matter)
- [x] Great Powers start with mutual embassies (includes trade consulate functionality)
- [x] Unit tests: relationship score calculations from various actions

### Diplomatic Infrastructure
- [x] **Trade Consulate** — costs $500; must be established before trading with a Minor Nation
- [x] **Embassy** — costs $5,000; must be established before signing treaties with a Minor Nation
- [x] Great Power ↔ Great Power: embassies exist from game start
- [x] Great Power → Minor Nation: must build consulate, then embassy
- [x] Unit tests: infrastructure prerequisite validation

### Treaty Types (5 types)
- [x] **Non-Aggression Pact** (Great Power ↔ Minor Nation only)
  - [x] Minor Nation requests help if attacked
  - [x] Honoring the request: Minor Nation becomes a colony of the Great Power
  - [x] Refusing: diplomatic penalty
  - [x] Unit tests: pact trigger on attack, colony incorporation
- [x] **Alliance** (Great Power ↔ Great Power only)
  - [x] When one ally enters war, the other is expected to join
  - [x] Refusing to honor alliance: alliance broken + diplomatic standing penalty
  - [x] Negotiating separate peace: also breaks alliance + penalty
  - [x] Unit tests: alliance obligation triggers, penalty calculations
- [x] **Request to Join Empire** (Minor Nation → Great Power)
  - [x] Triggered when a Minor Nation's relationship with a Great Power is sufficiently high
  - [x] Minor Nation voluntarily incorporates into the empire
  - [ ] Unit tests: voluntary incorporation threshold
- [x] **Peace Treaty**
  - [x] Ends active war between two nations
  - [ ] Separate peace (without allies) damages diplomatic standing
  - [ ] Unit tests: war termination, standing impacts
- [x] **War Declaration**
  - [x] "The only treaty which may not be refused"
  - [x] Triggers alliance obligations for all allies of both sides
  - [x] Unit tests: war declaration cascade through alliances

### Diplomatic Actions
- [x] **Cash Grants** — direct money transfer to improve relations
- [ ] **Trade Subsidies** — increase export prices, decrease import costs for a Minor Nation
  - [ ] Ctrl+click on Minor Nation auto-calculates necessary subsidy to become preferred partner
  - [ ] Subsidy calculation algorithm
- [x] **Treaty Proposals** — propose any applicable treaty type
- [x] **War Declaration** — initiate hostilities
- [ ] Unit tests: grant and subsidy effects on relationship scores

### Diplomatic Standing
- [x] Global standing value per Great Power (affects all diplomatic interactions)
- [x] Reduced by: breaking alliances, refusing pact obligations, separate peace treaties
- [ ] Impacts: treaty acceptance probability, Minor Nation governor voting, trade willingness
- [ ] AI nations factor standing into their decisions
- [x] Unit tests: standing reduction from various violations
- [ ] Unit tests: standing impact on treaty acceptance probability

### Council of Governors Voting
- [ ] Each province has a governor who votes
- [ ] Minor Nation governors favor powers offering beneficial trade
- [ ] Great Power governors favor militarily strong nations
- [ ] Election held every decade (every 40 turns)
- [ ] Two-thirds majority = victory
- [ ] If no majority by 1915 (turn 400), most governors wins
- [ ] Unit tests: vote counting, majority calculation
- [ ] Unit tests: governor preference calculations (trade-based, military-based)

### Verification Strategy
- [x] **Unit tests**: Run test suite — all diplomacy tests pass
- [ ] **Integration test**: Simulate a full diplomatic lifecycle — build consulate → embassy → pact → voluntary incorporation
- [ ] **Integration test**: War declaration → alliance cascade → peace treaty → standing penalty
- [ ] **Scenario test**: Set up a game state where a Great Power should win the Council vote → verify victory
- [ ] **AI test**: AI Minor Nations respond correctly to trade/grants (relationship improves, eventually volunteer to join)
- [ ] **Edge case tests**: All possible alliance/war/pact interaction combinations
