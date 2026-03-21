# 29 — Web Frontend (WASM + TypeScript)

> **Architecture**: Rust domain compiled to WASM via `wasm-bindgen`, TypeScript/React UI,
> HTML5 Canvas for hex map rendering.

## Overview

The web frontend compiles the domain crate to WebAssembly and wraps it with a TypeScript
React application. The game logic runs entirely in WASM (same Rust code as CLI and Bevy
frontends). The UI is built with React components for panels and HTML5 Canvas for the hex map.

```
┌─────────────────────────────────────────────┐
│           TypeScript / React UI             │
│  React components: HUD, panels, menus       │
│  HTML5 Canvas: hex map rendering            │
│  Event handlers: click, hover, keyboard     │
├─────────────────────────────────────────────┤
│         wasm-bindgen Bridge                 │
│  Exposed functions: new_game, process_turn  │
│  Serialized data: JSON game state → JS      │
├─────────────────────────────────────────────┤
│         Rust Domain (WASM)                  │
│  Same domain crate, compiled to wasm32      │
│  Zero changes to game logic                 │
└─────────────────────────────────────────────┘
```

## Checklist

### WASM Bridge Crate
- [ ] Create `crates/wasm-bridge/` crate with `wasm-bindgen` dependency
- [ ] `Cargo.toml`: `crate-type = ["cdylib"]`, depends on `domain`, `serde`, `serde_json`, `wasm-bindgen`
- [ ] Expose `wasm_new_game(map_key: &str, difficulty: u8, nation_index: usize) -> String` — returns JSON game state
- [ ] Expose `wasm_process_turn(game_json: &str) -> String` — accepts/returns JSON
- [ ] Expose `wasm_get_map_data(game_json: &str) -> String` — returns hex map as JSON for rendering
- [ ] Expose `wasm_get_nation_info(game_json: &str, nation_id: u32) -> String`
- [ ] Expose `wasm_research_tech(game_json: &str, tech_name: &str) -> String`
- [ ] Expose `wasm_get_available_techs(game_json: &str) -> String`
- [ ] Expose `wasm_declare_war(game_json: &str, target_id: u32) -> String`
- [ ] Expose `wasm_make_peace(game_json: &str, target_id: u32) -> String`
- [ ] Expose `wasm_build_unit(game_json: &str, unit_type: &str) -> String`
- [ ] Expose `wasm_get_scenarios() -> String` — returns available scenarios as JSON
- [ ] Build with `wasm-pack build --target web`
- [ ] Verify WASM binary size < 5MB
- [ ] Unit tests: each exposed function returns valid JSON

### TypeScript Project Setup
- [ ] Create `web/` directory with Vite + React + TypeScript
- [ ] `npm create vite@latest web -- --template react-ts`
- [ ] Install dependencies: `npm install`
- [ ] Configure Vite to serve WASM module from `crates/wasm-bridge/pkg/`
- [ ] Create `web/src/wasm.ts` — typed wrapper around WASM functions
- [ ] TypeScript interfaces matching Rust structs (GameState, Nation, Tile, etc.)
- [ ] Development server: `npm run dev` serves at localhost:5173
- [ ] Build: `npm run build` produces static files for deployment

### Hex Map Canvas Renderer
- [ ] `web/src/components/HexMap.tsx` — Canvas-based hex map
- [ ] Render hex tiles as colored polygons (not squares — actual hexagons)
- [ ] Nation territory colors matching the CLI/Bevy colors
- [ ] Terrain type labels on each hex
- [ ] Capital markers (★)
- [ ] Province borders drawn between different owners
- [ ] Sea tiles in blue
- [ ] Pan: click-and-drag to move the view
- [ ] Zoom: mouse wheel to zoom in/out
- [ ] Hover: highlight tile under cursor, show tooltip
- [ ] Click: select tile, show detail panel
- [ ] Performance: only render visible tiles (viewport culling)
- [ ] Responsive: fill the browser viewport

### UI Components (React)
- [ ] `TopBar.tsx` — nation name, turn, treasury, end-turn button
- [ ] `InfoPanel.tsx` — tile/province details on hover/click
- [ ] `NationList.tsx` — sidebar with all nations and scores
- [ ] `TechTree.tsx` — available/researched technologies with research button
- [ ] `Warehouse.tsx` — resource/material/goods inventory
- [ ] `BuildingsPanel.tsx` — capital buildings and capacities
- [ ] `ArmyPanel.tsx` — military units with stats
- [ ] `DiplomacyPanel.tsx` — diplomatic relations, treaties, war/peace buttons
- [ ] `TradePanel.tsx` — Minor Nation trade partners and offerings
- [ ] `NewspaperModal.tsx` — Imperial Times popup between turns
- [ ] `ScoreBoard.tsx` — Great Power rankings
- [ ] `GameSetup.tsx` — new game: map key, difficulty, nation selection, scenario choice
- [ ] `MainMenu.tsx` — start screen with New Game / Load / Scenarios
- [ ] `GameOverModal.tsx` — victory/defeat screen with final scores

### Game Loop Integration
- [ ] Game state stored in React state (useState or Zustand)
- [ ] `endTurn()` calls WASM `process_turn`, updates React state
- [ ] Turn report parsed and displayed in newspaper modal
- [ ] Map re-renders after state change
- [ ] All player actions (research, build, attack, etc.) call WASM functions
- [ ] Action results update game state and trigger UI refresh
- [ ] Save/load via browser localStorage (serialize game state JSON)

### Keyboard & Input
- [ ] Spacebar: end turn
- [ ] Escape: close modals
- [ ] WASD/arrows: pan map
- [ ] Mouse wheel: zoom
- [ ] Click: select tile/unit
- [ ] Right-click: context menu (build, move, attack)

### Styling & Polish
- [ ] 19th-century aesthetic: sepia tones, serif fonts, parchment textures
- [ ] Responsive layout: sidebar + main map area
- [ ] Dark mode support
- [ ] Loading spinner while WASM initializes
- [ ] Toast notifications for game events

### Build & Deployment
- [ ] `scripts/build-web.sh`: builds WASM + web app
- [ ] Output: `web/dist/` with static files (HTML + JS + WASM)
- [ ] Deployable to any static host (GitHub Pages, Netlify, Vercel)
- [ ] Total bundle size < 10MB
- [ ] Works in Chrome, Firefox, Safari, Edge

### Verification Strategy
- [ ] **WASM build**: `wasm-pack build` succeeds
- [ ] **Bridge test**: Each WASM function returns valid JSON
- [ ] **Web dev server**: `npm run dev` starts and loads the game
- [ ] **Map renders**: Hex tiles visible with correct colors
- [ ] **Turn processing**: End turn → state updates → UI refreshes
- [ ] **Full game**: Play from 1815 to 1825 in the browser without errors
- [ ] **Bundle size**: Production build < 10MB total
- [ ] **Cross-browser**: Tested in Chrome + Firefox
