# 19 — UI — Tactical Battle Screen

> **STATUS: DEFERRED** — All battles are auto-resolved with the combat engine
> (terrain/fort bonuses, firepower, casualties, medals). Tactical hex-based battle
> mode deferred to post-MVP. The auto-resolve system produces detailed battle reports.

## Overview

When land combat occurs, the player can choose to fight tactically on a hex-based
battlefield or auto-resolve. The tactical screen is a separate mode from the strategic map.

## Checklist

### Battle Screen Layout
- [ ] Hex battlefield generated from province terrain
- [ ] Attacker deploys on one side, defender on the other
- [ ] Fort structures rendered if present (with destructible sections)
- [ ] Terrain features visible (hills, forests, open ground)
- [ ] Unit sprites with health bars, medal icons, and type indicators
- [ ] Turn order / initiative display panel
- [ ] Current unit highlight with movement range overlay
- [ ] Fire range overlay when unit selected
- [ ] Unit info panel: type, health, medals, firepower, movement, range
- [ ] Battle log: scrollable text of combat events

### Deployment Phase
- [ ] Player places units on designated deployment tiles
- [ ] Auto-deploy option available (AI places units)
- [ ] Counter-attack deployment: player should place faster units in front (auto-deploy doesn't do this)
- [ ] "Cancel" button for deployment (original had a bug here — we reproduce the UI but fix the bug)
- [ ] Deployment zone highlighted
- [ ] Drag-and-drop or click-to-place unit positioning
- [ ] Unit tests: deployment zone calculations, valid placement validation

### Combat Phase — Player Turn
- [ ] Select a unit → show movement range (highlighted hexes)
- [ ] Move unit (click destination hex within range)
- [ ] Fire at enemy (click enemy unit within firing range)
- [ ] Movement + fire per combat turn (move then fire, or fire without moving)
- [ ] Opportunity fire: defending units automatically fire when enemy enters their range
- [ ] Visual feedback: damage numbers, hit/miss indicators, health bar changes
- [ ] "S" key: skip all units until next Sapper (for siege convenience)
- [ ] Skip interrupted if any unit is attacked
- [ ] "End unit turn" button to pass without acting
- [ ] Unit tests: movement validation, fire range validation, opportunity fire triggers

### Combat Phase — AI Turn
- [ ] AI units move and fire with visible animations
- [ ] AI decision-making for unit actions (targeting, positioning, retreating)
- [ ] Speed control: adjust AI turn animation speed (fast / normal / instant)
- [ ] Unit tests: AI combat decisions produce valid actions

### Sapper Visualization
- [ ] Tunnel progress shown as dug hex path toward fort
- [ ] Half movement = one tunnel space dug
- [ ] Stationary sapper = two tunnel spaces
- [ ] Explosion animation when tunnel reaches fort wall
- [ ] Fort section destruction visible (gap in wall)
- [ ] Unit tests: tunnel rendering matches game logic state

### Artillery Visualization
- [ ] Artillery firing arcs and range circles
- [ ] Heavy artillery destroying fort sections (alternative to sappers)
- [ ] Splash/impact effects on target hexes
- [ ] Siege progression visible over multiple combat turns

### Battle Resolution Display
- [ ] Victory / defeat announcement
- [ ] Casualty summary: units lost, units damaged, medals earned
- [ ] Province conquest notification if attacker won
- [ ] Retreat animation for losing side
- [ ] Return to strategic map after battle concludes

### Auto-Resolve Option
- [ ] Player can skip tactical battle entirely
- [ ] AI resolves the battle using the same combat engine
- [ ] Battle report shown with outcome summary
- [ ] Configurable in game preferences (always auto-resolve, always tactical, ask each time)
- [ ] Unit tests: auto-resolve produces same result as step-by-step tactical resolution (given same RNG seed)

### Verification Strategy
- [ ] **Unit tests**: All battle UI state machines pass
- [ ] **Render test**: Battle screen renders correctly with various unit compositions and terrain types
- [ ] **Input test**: All combat actions (move, fire, skip, sapper tunnel, deploy) produce correct state changes
- [ ] **AI turn test**: AI completes its combat turn without errors; all actions are valid
- [ ] **Siege test**: Sapper tunnels to fort → fort section destroyed → units can advance through gap
- [ ] **Auto-resolve parity**: Same battle auto-resolved vs. manually played with same RNG → same outcome
- [ ] **Performance test**: Battle with maximum unit count (both sides full) renders and processes at 60fps
