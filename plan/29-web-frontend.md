# 29 — Web Frontend: UI, Accessibility & Localization

> Combines former plans 18 (UI Screens), 25 (Accessibility & Localization), and 29 (Web Frontend).

> **Architecture**: Rust domain compiled to WASM via `wasm-bindgen`, TypeScript/React UI,
> HTML5 Canvas for hex map rendering.

## Overview

The web frontend compiles the domain crate to WebAssembly and wraps it with a TypeScript
React application. The game logic runs entirely in WASM (same Rust code as CLI and Bevy
frontends). The UI is built with React components for panels and HTML5 Canvas for the hex map.

The original game had 5 in-game screens per turn (Map, Transport, Industry, Trade, Diplomacy),
a main menu, a game setup screen, and a newspaper between turns. The web frontend reproduces
all of these.

```
┌─────────────────────────────────────────────────────────────────┐
│             TypeScript / React UI                               │
│  React components: HUD, panels, menus                          │
│  HTML5 Canvas: hex map rendering                               │
│  Event handlers: click, hover, keyboard                        │
├─────────────────────────────────────────────────────────────────┤
│           wasm-bindgen Bridge                                   │
│  Exposed functions: new_game, process_turn                      │
│  Serialized data: JSON game state → JS                         │
├─────────────────────────────────────────────────────────────────┤
│           Rust Domain (WASM)                                    │
│  Same domain crate, compiled to wasm32                         │
│  Zero changes to game logic                                    │
└─────────────────────────────────────────────────────────────────┘
```

---

## WASM Bridge Crate

- [x] Create `crates/wasm-bridge/` crate with `wasm-bindgen` dependency
- [x] `Cargo.toml`: `crate-type = ["cdylib"]`, depends on `domain`, `serde`, `serde_json`, `wasm-bindgen`
- [x] Expose `wasm_new_game(map_key, difficulty, nation_index) -> String`
- [x] Expose `wasm_process_turn(game_json) -> String`
- [x] Expose `wasm_get_map_data(game_json) -> String`
- [x] Expose `wasm_research_tech(game_json, tech_name) -> String`
- [x] Expose `wasm_get_available_techs(game_json) -> String`
- [x] Expose `wasm_get_scenarios() -> String`
- [x] Build with `wasm-pack build --target web` (binary 782KB — well under 5MB target)
- [ ] Expose `wasm_get_nation_info(game_json, nation_id) -> String`
- [ ] Expose `wasm_declare_war(game_json, target_id) -> String`
- [ ] Expose `wasm_make_peace(game_json, target_id) -> String`
- [ ] Expose `wasm_build_unit(game_json, unit_type) -> String`
- [ ] Expose `wasm_get_economy_data(game_json) -> String` — warehouse, production, workers
- [ ] Expose `wasm_get_diplomacy_data(game_json) -> String` — relations, treaties
- [ ] Expose `wasm_get_military_data(game_json) -> String` — units, armies
- [ ] Expose `wasm_build_infrastructure(game_json, tile_q, tile_r, infra_type) -> String`
- [ ] Expose `wasm_assign_workers(game_json, assignments) -> String`
- [ ] Expose `wasm_set_trade_offers(game_json, offers) -> String`
- [ ] Expose `wasm_save_game(game_json) -> String`
- [ ] Expose `wasm_load_game(save_json) -> String`
- [ ] Unit tests: each exposed function returns valid JSON

## TypeScript Project Setup

- [x] Create `web/` directory with Vite + React + TypeScript
- [x] Install dependencies, configure Vite to serve WASM from `crates/wasm-bridge/pkg/`
- [x] Create `web/src/wasm.ts` — typed wrapper around WASM functions
- [x] Dev server (`npm run dev`) and production build (`npm run build`) working
- [ ] TypeScript interfaces matching Rust structs — replace `any` with proper types for `GameState`, `Nation`, `TurnReport`, `TechData`
- [ ] Add React error boundary — WASM crash currently shows blank white screen

---

## Hex Map Canvas Renderer

- [x] `web/src/components/HexMap.tsx` — Canvas-based hex map
- [x] Hex tiles as colored polygons (pointy-top hexagons)
- [x] Nation territory colors with political/terrain view toggle
- [x] 3-tier borders: country (thick), province (medium), hex (thin)
- [x] Capital markers (★ country, ○ provincial)
- [x] Infrastructure icons: railroad tracks, depot squares, anchor (port), tower (fort)
- [x] Sea tiles in blue
- [x] Pan (click-drag), zoom (mouse wheel)
- [x] Hover shows tile info in side panel
- [x] Click to select tile (note: hover takes priority — selected tile invisible while hovering)
- [x] Nation name labels on landmasses via BFS connected-component detection
- [ ] Fix canvas not filling map area — large dark empty space below hex grid at desktop resolution
- [ ] Viewport culling — currently renders all tiles every frame regardless of visibility
- [ ] Hover detection uses O(n) scan over all tiles — replace with pixel-to-hex math
- [ ] Minimap overlay for navigation on zoomed-in views
- [ ] Terrain type labels on hex tiles (currently only visible via hover panel)
- [ ] Unit rendering — military units on the map with sprites/icons
- [ ] Civilian specialist rendering on map tiles
- [ ] Unit movement orders: click unit, click destination
- [ ] Right-click context menus for common actions (build, move, attack)
- [ ] Infrastructure construction: select engineer → build menu overlay
- [ ] Ctrl+click zoom icon → display map key

---

## Game Screens

### Main Menu
- [ ] Start screen: New Game, Load Scenario, Load Game, Preferences, Quit
- [ ] 19th-century study aesthetic

### Game Setup
- [ ] `GameSetup.tsx` — currently hardcoded to `newGame('imperialism', 2, 0)`, `getScenarios()` exists but unused
- [ ] Map type selection: Random or Scenario
- [ ] Difficulty selection: Introductory / Easy / Normal / Hard / NOI
- [ ] Nation selection: choose from available Great Powers
- [ ] Map key input: enter specific map key (32 chars, case-sensitive)
- [ ] Player name entry
- [ ] Start game button

### Map Screen (Screen 1)
- [x] Full hex map rendering with terrain and nation colors
- [x] Province borders clearly visible
- [x] Pan and zoom controls
- [x] Tile info panel on hover/click (terrain, resources, improvements)
- [ ] Civilian specialist deployment on map
- [ ] Military unit orders: move, attack, patrol
- [ ] Province info panel (owner, settlement level, garrison)
- [ ] Hotkeys: F5 (civilians list), F6 (army list), F7 (ships list)

### Transport Screen (Screen 2)
- [ ] Freight car inventory display
- [ ] Resource allocation sliders per resource type
- [ ] Total capacity vs. total production comparison
- [ ] Build freight cars button (with cost)
- [ ] Preview next turn's deliveries

### Industry Screen (Screen 3)
- [ ] Capital city building layout
- [ ] Mill and factory production assignment
- [ ] Worker recruitment panel (Capitol — needs food + clothing + furniture)
- [ ] Worker training panel (Trade School — untrained → trained → expert)
- [ ] Specialist creation panel (University — expert → specialist)
- [ ] Building expansion interface (cost: 1 lumber + 1 steel per capacity)
- [ ] Warehouse view — current inventory + incoming
- [ ] Armory — military unit construction
- [ ] Shipyard — ship construction
- [ ] Railyard — freight car construction

### Trade Screen (Screen 4)
- [ ] List of trade partners (Minor Nations with consulates)
- [ ] Available goods per partner with prices
- [ ] Offer/bid creation interface
- [ ] Cargo capacity display (merchant marine holds)
- [ ] Subsidy management per partner
- [ ] Expected revenue preview

### Diplomacy Screen (Screen 5)
- [ ] World overview map (political colors)
- [ ] Nation info panels: relationship score, treaty status, military estimate
- [ ] Build Trade Consulate ($500), Build Embassy ($5,000)
- [ ] Propose Treaty dropdown: pact, alliance, peace
- [ ] Declare War button (with confirmation dialog)
- [ ] Cash Grant interface
- [ ] Council of Governors overview: current vote projection

### Screen Navigation
- [ ] Tab bar or navigation buttons: Map → Transport → Industry → Trade → Diplomacy
- [ ] Hotkeys for screen switching (F1–F5 or number keys)

### Newspaper (between turns)
- [x] "The Imperial Times" masthead with current date
- [x] Headlines color-coded by category (war, diplomacy, growth, trade, etc.)
- [x] Click Continue to dismiss
- [ ] Group headlines by nation; separate player events from AI events
- [ ] Scrollable when many items
- [ ] Click-to-dismiss backdrop

### End-of-Game
- [ ] `GameOverModal.tsx` — victory/defeat screen with final scores
- [ ] `ScoreBoard.tsx` — Great Power rankings

---

## UI Components (React)

- [x] Top bar — nation name, turn/year, treasury, provinces, Tech button, End Turn button (inline in App.tsx)
- [x] Tile info panel — terrain, province, owner, level, capital/railroad/fort (inline in App.tsx)
- [x] Nation list sidebar — Great Powers with province counts (inline in App.tsx)
- [x] Tech research modal — available techs with cost and Research button (inline in App.tsx)
- [ ] **Refactor**: Extract inline components into separate files (TopBar, InfoPanel, NationList, NewspaperModal, TechModal)
- [ ] End Turn confirmation dialog when player has unspent orders
- [ ] Tooltip system for all interactive elements
- [ ] Notification system / toast for important events

## Game Loop Integration

- [x] Game state stored in React `useState`
- [x] `endTurn()` calls WASM `process_turn`, updates state, shows newspaper
- [x] Map re-renders after state change
- [ ] Player actions beyond End Turn + Tech — economy, military, diplomacy, infrastructure all missing (human is currently a spectator)
- [ ] Save/load via browser localStorage
- [ ] Undo last action (revert to previous game state JSON)

---

## Keyboard & Input

- [x] Mouse wheel: zoom
- [x] Click: select tile
- [ ] Spacebar: end turn
- [ ] Escape: close modals
- [ ] WASD/arrows: pan map
- [ ] Right-click: context menu (build, move, attack)
- [ ] Full keyboard navigation for all screens (tab order, arrow keys, hotkeys)
- [ ] Tab navigation with visible focus indicators on all interactive elements
- [ ] Hotkey reference card (accessible in-game)
- [ ] Remappable controls
- [ ] Mouse-only play fully supported (no keyboard-only actions)
- [ ] Keyboard-only play fully supported (no mouse-only actions)
- [ ] Confirm destructive actions (war declarations, treaty breaking) with dialog
- [ ] Touch support: pinch-to-zoom, tap-to-select, two-finger drag to pan
- [ ] Gamepad support (stretch goal)

---

## Accessibility

### Immediate Fixes (Lighthouse failures)
- [ ] Fix color contrast: "Hover over a tile" hint `#666` on `#161625` (3.11:1 → needs 4.5:1)
- [ ] Add `<main>` landmark wrapping the game area
- [ ] Add `aria-label` to canvas element (game view invisible to screen readers)
- [ ] Add `<meta name="description">` to `index.html`
- [ ] Visible focus styles on all buttons and interactive elements

### Visual Accessibility
- [ ] Color-blind modes: Deuteranopia, Protanopia, Tritanopia filters
- [ ] Nation colors distinguishable in all modes (use patterns/icons as supplements)
- [ ] High-contrast mode: enhanced borders and text contrast
- [ ] Adjustable font size: small / medium / large / extra-large
- [ ] UI scaling: 100% / 125% / 150% / 200%
- [ ] Minimap uses patterns in addition to colors for nation identification
- [ ] Terrain tiles distinguishable by icon/pattern, not just color

### Screen Reader Support (Stretch Goal)
- [ ] ARIA labels for all UI elements
- [ ] Descriptive text for map tiles, units, buildings on focus
- [ ] Battle narration mode: text description of combat events
- [ ] Announcement of turn changes, combat results, diplomatic events

### Audio Accessibility
- [ ] Independent volume controls: Master, BGM, SFX
- [ ] Visual indicators for all audio cues
- [ ] Option to disable screen shake / flashing effects

---

## Localization

- [ ] String table system: all UI text from localization files
- [ ] `data/localization/{locale}.json` — key-value pairs
- [ ] Placeholder substitution: `"Turn {turn_number} — Year {year}"`
- [ ] Pluralization support: `"1 unit" / "3 units"`
- [ ] Date/number formatting per locale
- [ ] Font fallback for extended character sets (CJK, Cyrillic)
- [ ] English (default, complete)
- [ ] Framework for adding: French, German, Spanish, Portuguese, Russian, Chinese, Japanese
- [ ] Community-editable locale files in mod format
- [ ] Missing translation fallback: English string + warning icon
- [ ] RTL text support (stretch goal — Arabic, Hebrew)

---

## Responsive Design

- [ ] Mobile breakpoint: stack sidebar below map when viewport < 768px
- [ ] Top bar: collapse into hamburger menu or icon-only mode on narrow viewports
- [ ] Test at 375px, 768px, 1024px, 1440px, 4K viewports
- [ ] Text remains readable at all supported resolutions

## Styling & Polish

- [x] 19th-century aesthetic: dark theme, goldenrod accents, Georgia serif font
- [ ] Move inline styles to CSS modules — all styles currently inline `React.CSSProperties` in App.tsx
- [ ] CSS variables for theme colors (enable dark/light mode toggle)
- [ ] Loading spinner while WASM initializes (currently just text)
- [ ] Toast notifications for game events

## Build & Deployment

- [x] `scripts/build-web.sh`: builds WASM + web app
- [x] Output: `web/dist/` with static files (HTML + JS + WASM)
- [ ] Deployable to static host (GitHub Pages, Netlify, Vercel) — needs testing
- [ ] Total bundle size < 10MB
- [ ] Works in Chrome, Firefox, Safari, Edge — needs cross-browser testing

---

## Verification Strategy

- [x] **WASM build**: `wasm-pack build` succeeds
- [x] **Web dev server**: `npm run dev` starts and loads the game
- [x] **Map renders**: Hex tiles visible with correct colors and borders
- [x] **Turn processing**: End turn → state updates → newspaper → UI refreshes
- [ ] **Bridge test**: Each WASM function returns valid JSON
- [ ] **Full game**: Play 1815–1825 in the browser without errors
- [ ] **All 5 screens**: Navigate Map → Transport → Industry → Trade → Diplomacy, verify correct data
- [ ] **Input test**: Click every interactive element → correct action triggered
- [ ] **Keyboard test**: Tab through all elements; every action reachable without mouse
- [ ] **Bundle size**: Production build < 10MB total
- [ ] **Cross-browser**: Chrome + Firefox + Safari
- [ ] **Lighthouse**: Accessibility ≥ 95, Best Practices = 100 (currently: A11y 86, BP 100)
- [ ] **Responsive**: Playable at 768px tablet and 1440px desktop
- [ ] **Color-blind test**: Enable each mode → verify all nations distinguishable
- [ ] **Font scaling test**: Each size → no text overflow or clipping
- [ ] **Locale test**: Switch locale → no missing strings, no layout breaks
