# 18 — UI — Main Screens

## Overview

The game has a main menu, 5 in-game screens per turn, and various overlay panels.
The original ran at 640×480; the remake should support modern resolutions while
preserving the functional layout.

## Checklist

### Main Menu Screen
- [ ] **New Game** (globe icon) — start random map game
- [ ] **Load Scenario** (book icon) — select historical scenario or tutorial
- [ ] **Load Game** (ship in bottle icon) — load saved game
- [ ] **Preferences** (pen/ink icon) — sound, warnings, tactical battle auto-resolve toggle
- [ ] **High Scores** (trophy cabinet icon) — view score history
- [ ] **Multiplayer** (telephone icon) — network game setup
- [ ] **Quit** (doors icon) — exit game
- [ ] Art/theme: 19th-century study aesthetic
- [ ] Unit tests: menu navigation state machine

### Game Setup Screen
- [ ] Map type selection: Random or Scenario
- [ ] Difficulty selection: Introductory / Easy / Normal / Hard / NOI
- [ ] Nation selection: choose from available Great Powers
- [ ] Map key input: Ctrl+click globe to enter specific map key (32 chars, case-sensitive)
- [ ] Player name entry
- [ ] Map preview / regeneration
- [ ] Start game button
- [ ] Unit tests: setup validation (valid selections before start)

### Map Screen (F1 / Screen 1)
- [ ] Full hex map rendering with terrain sprites and nation colors
- [ ] Province borders clearly visible
- [ ] Pan and zoom controls
- [ ] Minimap overlay
- [ ] Unit placement and movement (drag or click-to-order)
- [ ] Civilian specialist deployment
- [ ] Military unit orders: move, attack, patrol
- [ ] Infrastructure construction (engineer selected → build menu)
- [ ] Tile info panel on hover/click (terrain, resources, improvements, units)
- [ ] Province info panel (owner, settlement level, garrison)
- [ ] Right-click context menus for common actions
- [ ] Hotkeys: F5 (civilians list), F6 (army list), F7 (ships list)
- [ ] Ctrl+click zoom icon → display map key
- [ ] Unit tests: map interaction state (select unit, issue order, confirm)

### Transport Screen (Screen 2)
- [ ] Freight car inventory display
- [ ] Resource allocation sliders per resource type
- [ ] Total capacity vs. total production comparison
- [ ] Build freight cars button (with cost display)
- [ ] Preview next turn's deliveries
- [ ] Unit tests: slider allocation produces correct transport orders

### Industry Screen (Screen 3)
- [ ] Capital city building layout (south-east corner for factories)
- [ ] Mill and factory production assignment
- [ ] Worker recruitment panel (Capitol — needs food + clothing + furniture)
- [ ] Worker training panel (Trade School — untrained → trained → expert)
- [ ] Specialist creation panel (University — expert → specialist)
- [ ] Building expansion interface (cost: 1 lumber + 1 steel per capacity)
- [ ] Warehouse view — current inventory + incoming
- [ ] Armory — military unit construction
- [ ] Shipyard — ship construction
- [ ] Railyard — freight car construction
- [ ] Unit tests: industry screen data calculations

### Trade Screen (Screen 4)
- [ ] List of trade partners (Minor Nations with consulates)
- [ ] Available goods per partner with prices
- [ ] Offer creation: select good, quantity, price
- [ ] Bid creation: select resource, quantity, max price
- [ ] Cargo capacity display (merchant marine holds)
- [ ] Subsidy management per partner
- [ ] Ctrl+click Minor Nation → auto-calculate optimal subsidy
- [ ] Expected revenue preview
- [ ] Diplomatic relationship indicator per partner
- [ ] Unit tests: trade order validation

### Diplomacy Screen (Screen 5)
- [ ] World overview map (political map with nation colors)
- [ ] Nation info panels: relationship score, treaty status, military estimate
- [ ] Build Trade Consulate button ($500)
- [ ] Build Embassy button ($5,000)
- [ ] Propose Treaty dropdown: pact, alliance, peace
- [ ] Declare War button (with confirmation dialog)
- [ ] Cash Grant interface (amount input)
- [ ] Council of Governors overview: current vote projection
- [ ] Diplomatic standing display
- [ ] Unit tests: diplomacy action validation

### Newspaper Screen (between turns)
- [ ] "Imperial Times" masthead with current date
- [ ] Scrollable news items
- [ ] Items categorized: technology, military, diplomatic, flavor
- [ ] Click-to-dismiss or auto-advance option
- [ ] Unit tests: newspaper content assembly from turn events

### Shared UI Elements
- [ ] Turn counter / year display
- [ ] Treasury display
- [ ] End Turn button (with confirmation for unfinished orders)
- [ ] Screen navigation tabs/buttons (Map → Transport → Industry → Trade → Diplomacy)
- [ ] Notification system for important events
- [ ] Tooltip system for all interactive elements
- [ ] Unit tests: shared component state management

### Resolution Scaling
- [ ] Support for modern resolutions (1080p, 1440p, 4K)
- [ ] UI scaling factor configurable
- [ ] Hex tile size adjusts with zoom level
- [ ] Text remains readable at all supported resolutions
- [ ] Functional layout preserved regardless of resolution

### Verification Strategy
- [ ] **Unit tests**: All UI state machines and data calculations pass
- [ ] **Render test**: Each screen renders without errors at 1080p, 1440p, 4K resolutions
- [ ] **Navigation test**: Cycle through all 5 screens → verify correct data displayed on each
- [ ] **Input test**: Click every interactive element → verify correct action triggered
- [ ] **Accessibility test**: Tab navigation works for all interactive elements; screen reader labels present
- [ ] **Screenshot comparison**: Capture reference screenshots; detect visual regressions on changes
