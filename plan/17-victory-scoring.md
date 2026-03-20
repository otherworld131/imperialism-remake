# 17 — Victory & Scoring

## Overview

Victory is achieved through the Council of Governors — either by two-thirds majority vote
or by having the most governors when the game reaches 1915. Score tracks overall development.

## Checklist

### Council of Governors
- [ ] Each province has one governor
- [ ] Total governors = total provinces in the game (7×8 + 16×4 = 120 in standard game)
- [ ] Conquered provinces' governors belong to the conqueror
- [ ] Minor Nation governors' voting preference:
  - [ ] Favor powers offering beneficial trade (trade volume, subsidy level)
  - [ ] Favor powers with high diplomatic relationship scores
- [ ] Great Power governors' voting preference:
  - [ ] Favor the militarily strongest nations
- [ ] Election held every decade (every 40 turns): 1825, 1835, 1845, ..., 1915
- [ ] Two-thirds majority (≥ 80 out of 120) = immediate victory
- [ ] If no majority by 1915: most governors wins
- [ ] Unit tests: governor vote counting and majority calculation
- [ ] Unit tests: governor preference calculations (trade-based, military-based)
- [ ] Unit tests: decade boundary detection

### Score Components
- [ ] **Military size** — total firepower of all army units
- [ ] **Labor force** — total workers (untrained, trained, expert, specialist)
- [ ] **Transport networks** — total freight car capacity + railroad miles
- [ ] **Merchant marine** — total cargo capacity of merchant ships
- [ ] **Diplomatic standing** — accumulated standing value
- [ ] **Provinces controlled** — number of provinces in empire
- [ ] Score recalculated each turn
- [ ] High score table for completed games
- [ ] Unit tests: score calculation from game state
- [ ] Unit tests: score components independently testable

### Victory Types
- [ ] **Council Victory (Vote)** — achieve two-thirds majority at a decade election
- [ ] **Council Victory (Default)** — most governors at game end (1915)
- [ ] **Conquest Victory** — implied by controlling enough provinces to dominate the vote
- [ ] **Elimination** — when a Great Power loses all provinces (effectively eliminated, but game continues for others)
- [ ] Unit tests: each victory type detection

### Game End Conditions
- [ ] Two-thirds majority at any decade election
- [ ] Reaching 1915 Q1 (turn 401) — final scoring
- [ ] No early concession option (game plays to conclusion)
- [ ] Unit tests: game end detection at decade boundaries and 1915

### Victory Screen
- [ ] Display winning nation and victory type
- [ ] Show final Council of Governors vote breakdown
- [ ] Show score breakdown by component
- [ ] Show high score ranking
- [ ] Compare to previous games
- [ ] Unit tests: victory screen data assembly

### Verification Strategy
- [ ] **Unit tests**: Run test suite — all victory/scoring tests pass
- [ ] **Integration test**: Set up game state where one nation has ≥80 governors → verify victory triggered at decade boundary
- [ ] **Integration test**: Advance game to 1915 with no majority → verify most-governors winner
- [ ] **Score test**: Create known game state → verify score matches expected calculation
- [ ] **Governor preference test**: Nation A trades heavily with Minor Nation X → verify X's governor favors A
- [ ] **Full game test**: AI-only game runs to completion → verify exactly one winner, valid final state
