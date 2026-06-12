# Imperialism Remake

## Vision

A faithful, superlative remake of **Imperialism** (Frog City Software, 1997) — a turn-based
grand-strategy game set in the 19th-century Industrial Revolution. Players lead one of seven
Great Powers competing for world dominance through economics, diplomacy, and military conquest.

## Guiding Principles

> [!IMPORTANT]
> All player actions resolve over the end turn. Nothing should take effect immediately when clicked from the UI. If an action appears to apply instantly, treat that as a bug and fix the queue / end-turn resolution path instead of patching around it in the frontend.

| Principle | Detail |
|-----------|--------|
| **Architecture** | Hexagonal / Clean / Ports-and-Adapters — hard boundary between frontend and backend |
| **Language** | **Rust** (engine) + **Lua** (moddable game logic & scripting) |
| **Cross-platform** | Windows → macOS → Linux → browser (WASM). All from the same codebase. |
| **Player scope** | Single-player first; architecture accounts for networked multiplayer from day one |
| **Fidelity** | Every documented mechanic from the original is reproduced; bugs are *not* reproduced |
| **Quality bar** | Production-grade AI, comprehensive tests, mod support, accessibility |
| **Data-driven** | All game entities defined in data files (RON/JSON) + Lua scripts, not hardcoded |

## Technology Stack

| Layer | Technology | Role |
|-------|-----------|------|
| **Rust** | Core engine | Turn resolution, combat, pathfinding, hex math, ECS, networking, serialization |
| **Lua** (via `mlua`) | Scripting | Tech tree logic, AI behavior, balance tuning, scenario scripting, mod hooks, event handlers |
| **Bevy** / Godot GDExt | Frontend | Rendering, UI, input, audio — swappable without touching game logic |
| **RON / JSON** | Data | Unit stats, building defs, terrain defs, nation defs — static configuration |
| **serde** | Serialization | Save/load, networking payloads, data file parsing |

### Why Rust + Lua?

- **Rust** provides: zero-cost abstractions, no GC pauses, native speed, compiles to Windows/macOS/Linux/WASM, memory safety, fearless concurrency for AI parallel evaluation
- **Lua** provides: hot-reloadable scripts during development, safe sandboxed modding, proven in games (Factorio, Civilization, WoW), trivial to embed via `mlua`, runs everywhere Rust runs including WASM
- **Together**: engine performance where it matters + modder-friendly scripting where flexibility matters

### How Lua reaches the WASM build (build-time bake)

`mlua`'s vendored Lua 5.4 needs a C toolchain that doesn't compile to
`wasm32-unknown-unknown`, so the WASM crate strips the `lua` feature.
To keep "Lua holds the numbers" true on every platform, `crates/domain/build.rs`
runs `mlua` natively at build time, executes every script under `scripts/`,
and emits the resulting tables to `$OUT_DIR/lua_baked.json`. The
`lua_bridge::baked` module (compiled only when `lua` is OFF) embeds that
JSON via `include_str!` and deserializes it into the same `GameConfig` /
`LuaAiConfig` structs the runtime Lua loader produces.

Consequences worth remembering:
- Editing a `scripts/*.lua` file changes the WASM build only after a Rust
  rebuild (the `restart-web-server.sh` script already watches `scripts/`).
- Modders distributing a binary cannot tune Lua post-install — neither
  native nor WASM. Real runtime filesystem loading is a future card; the
  current bake is the bridge.
- AI personality *functions* (war/peace/treaty/tech decisions) are pure
  Rust now (`crates/domain/src/ai/lua_bridge.rs`). The thresholds those
  functions consult are tunables in `scripts/ai/*.lua`. Adding new
  per-personality logic means adding Rust code, not Lua functions.

## Frontend / Backend Boundary

The architecture enforces a **hard split** between frontend and backend. The backend is a
library crate that knows nothing about rendering, audio, or input. The frontend is one of
many possible consumers.

```
┌─────────────────────────────────────────────────────────────────┐
│                     FRONTEND (Presentation)                     │
│                                                                 │
│  Bevy / Godot GDExt / macroquad / WASM+canvas / headless CLI   │
│  Rendering, sprites, UI widgets, input handling, audio playback │
│                                                                 │
│  Calls into Application layer via typed commands & queries.     │
│  Receives view-model structs — never raw domain entities.       │
├─────────────────────────────────────────────────────────────────┤
│                  APPLICATION (Use-Cases)                        │
│                                                                 │
│  Commands: PlaceUnit, EndTurn, ProposeTreaty, SetTradeOffer     │
│  Queries:  GetMapView, GetNationSummary, GetTradePartners      │
│  Orchestrates domain logic, returns view models to frontend.    │
├─────────────────────────────────────────────────────────────────┤
│                   BACKEND (Domain Core)                         │
│                                                                 │
│  Pure Rust: game rules, combat resolution, economy, hex math   │
│  Embedded Lua VM: tech tree logic, AI scripts, mod hooks        │
│  ZERO knowledge of rendering, I/O, or platform                 │
│                                                                 │
│  This is a library crate. It can be driven by:                  │
│    • A Bevy game (desktop)                                      │
│    • A Godot frontend (via GDExtension FFI)                     │
│    • A headless test harness (CI, AI-only simulations)          │
│    • A WASM module (browser UI)                                 │
│    • A multiplayer server (turn resolution, no rendering)       │
├─────────────────────────────────────────────────────────────────┤
│                    INFRASTRUCTURE (Adapters)                    │
│                                                                 │
│  Save/Load (serde + bincode/JSON), Networking (tokio/quinn),   │
│  File system, Platform services                                 │
│  Implements traits defined by Domain/Application.               │
└─────────────────────────────────────────────────────────────────┘
```

### What lives where?

| Concern | Layer | Language |
|---------|-------|----------|
| Hex math, coordinate system | Domain | Rust |
| Combat resolution engine | Domain | Rust |
| Turn resolution pipeline | Domain | Rust |
| Production chain calculations | Domain | Rust |
| Pathfinding (A*, connectivity) | Domain | Rust |
| Tech tree definitions & logic | Domain | Lua scripts (loaded by Rust) |
| AI decision-making | Domain | Rust core; tunables in Lua (per personality) |
| Balance parameters | Domain | RON/JSON data + Lua overrides |
| Mod hooks (on_turn_start, on_combat_end, …) | Domain | Lua callbacks |
| Scenario scripting (events, triggers) | Domain | Lua |
| Map rendering, sprite management | Frontend | Rust (Bevy) or GDScript (Godot) |
| UI screens (5 game screens, menus) | Frontend | Rust (Bevy) or GDScript (Godot) |
| Audio playback, music transitions | Frontend | Rust (Bevy) or GDScript (Godot) |
| Input handling, hotkeys | Frontend | Rust (Bevy) or GDScript (Godot) |
| Save file I/O | Infrastructure | Rust (serde) |
| Network transport | Infrastructure | Rust (tokio) |
| Mod file discovery & loading | Infrastructure | Rust |

## Project Layout

```
imperialism-remake/
├── CLAUDE.md                  ← you are here
├── plan/                      ← implementation checklists (28 files, ~1,577 items)
├── crates/
│   ├── domain/                # Pure game logic — deps: only std + mlua
│   │   ├── src/
│   │   │   ├── hex/           # Coordinate system, spatial queries
│   │   │   ├── map/           # Map, terrain, provinces, sea zones
│   │   │   ├── economy/       # Resources, production, trade, transport
│   │   │   ├── military/      # Units, combat, naval
│   │   │   ├── diplomacy/     # Treaties, relations, council
│   │   │   ├── tech/          # Tech tree (Rust engine + Lua definitions)
│   │   │   ├── ai/            # AI framework (Rust engine + Lua strategies)
│   │   │   ├── turn/          # Turn processor, game loop
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── application/           # Use-cases, commands, queries — deps: domain
│   ├── infrastructure/        # Persistence, networking — deps: application, serde, tokio
│   └── presentation/          # UI, rendering, audio — deps: application, bevy/godot
├── scripts/                   # Lua game scripts
│   ├── tech/                  # Tech tree definitions & effects
│   ├── ai/                    # AI behavior scripts per personality
│   ├── scenarios/             # Scenario event scripts
│   └── mods/                  # Mod hook entry points
├── data/
│   ├── definitions/           # RON/JSON: units, ships, buildings, terrain, nations
│   ├── scenarios/             # Scenario configs (map + starting conditions)
│   ├── sprites/
│   ├── audio/
│   └── localization/
├── tests/
│   ├── unit/                  # Domain + application unit tests
│   ├── integration/           # Cross-crate integration tests
│   ├── architecture/          # Dependency rule fitness functions
│   └── simulation/            # Multi-turn AI-only game simulations
├── docs/adr/
├── tools/                     # Map editor, scenario editor
└── Cargo.toml                 # Workspace root
```

## Git Workflow

When working inside a git worktree instead of the main repository checkout,
always start by creating a new branch from `origin/main` in that worktree
before making code changes.

## Running Games

```bash
# Run N AI-only batch games (headless, no UI) — redirect JSON report to file
cargo run --release --bin imperialism -- --batch N > report.json

# Single interactive game
cargo run --release --bin imperialism -- [map_key] [nation_index]

# Native GUI (Bevy) — boots into the setup flow (config → preview →
# capital placement → campaign). Saves go to ./saves/ in the CLI-compatible
# formats, so GUI and CLI saves interchange freely.
cargo run --release --features gui --bin imperialism-gui

# Web frontend — rebuild WASM and (re)start the dev server.
# Default to --opt: optimized WASM is the right build for gameplay,
# runtime performance, and any user-facing testing.
./web/restart-web-server.sh --opt
# Faster rebuild for inner-loop UI/glue iteration only (unoptimized):
./web/restart-web-server.sh
# Then open http://localhost:43173
```

### Native GUI env flags

| Flag | Effect |
|------|--------|
| `HUMAN_GAME=1` | Skip setup, start a human game (seat 0) on the default 80×50 map |
| `OBSERVER_GAME=1` | Skip setup, start an observer (AI-only) game |
| `MAP_W=…` / `MAP_H=…` | Map size for the skip-setup shortcuts above |
| `WIDGET_GALLERY=1` | Overlay the debug widget gallery |
| `PERF_STATS=1` | FPS diagnostics + automated perf run (end turn, zoom-out pan, frame-pacing summary, exit) |
| `MAP_SCREENSHOT=<path>` | Capture the window after settling, then exit (`MAP_DEBUG_MODE`, `MAP_DEBUG_ZOOM`, `MAP_DEBUG_FOG`, `MAP_DEBUG_STRAIGHT`, `MAP_SCREENSHOT_FRAME` tweak the shot) |
| `M6_DEBUG`…`M10_DEBUG` | Scripted interaction drivers (units/trade/diplomacy/battles/setup flows) for screenshot verification — see `crates/presentation/src/app.rs` |

Linux/Wayland users can opt into the native Wayland backend with
`--features gui,wayland` (off by default; needs libwayland dev packages).
Windows needs no extra windowing feature.

GUI icons are loose 64×64 PNGs under `crates/presentation/assets/icons/`,
generated from handcrafted SVGs in `assets-src/icons/` via
`cargo run -p gen_assets` — see `assets-src/icons/MANIFEST.md` for the
icon-replacement workflow (quick PNG swap needs no recompile).

After implementing any plan that touches game logic, **run a short headless smoke test** to verify the full game loop works end-to-end, not just unit tests. Default to **`--batch 1 --batch-max-turns 20`** — one game, capped at 20 turns, finishes in well under a minute. A full 1-game run goes to 1915 (~70 turns) and 3 games take 3–6 minutes; only run that long form when you specifically need late-game state.

After any changes to Rust code that affect the web frontend, use `./web/restart-web-server.sh --opt` to rebuild the WASM bridge (optimized) and restart the dev server. Use the unoptimized `./web/restart-web-server.sh` only for fast inner-loop iteration on UI/glue code.

## Reference: Original Game Manual

The original Imperialism 1 manual PDF is at `docs/imperialism-1-manual.pdf` (120 pages).
Extract text with the provided utility:

```bash
# Single page
python3 tools/pdf_extract.py 17

# Page range
python3 tools/pdf_extract.py 15-20

# Search all pages for keyword
python3 tools/pdf_extract.py --search "naval landing"

# Search within a page range
python3 tools/pdf_extract.py --search "capital city" 10-30
```

Requires PyMuPDF (`pip install pymupdf`).

## Inspecting Batch Game Reports

After running `cargo run --release --bin imperialism -- --batch N`, use the
**`/batch-analyze`** skill (defined under `.claude/skills/batch-analyze/`)
to print focused summaries of the JSON output. The underlying tool is
`tools/batch_analyze.py` and is **data-driven** — any new field added to
`src/batch.rs::NationSnapshot` is discoverable via `--list-keys` without
code changes.

```bash
# Default: every section, year 1830 + last available.
python3 tools/batch_analyze.py /tmp/batch.json

# Targeted question (e.g. "is the AI buying Coal/Iron?"):
python3 tools/batch_analyze.py /tmp/batch.json --section trade --keys Coal,Iron

# Stream from cargo directly:
cargo run --release --bin imperialism -- --batch 3 | python3 tools/batch_analyze.py -
```

Sections: `summary`, `trade`, `warehouse`, `materials`, `paper`,
`bankruptcy`, plus `--section field --field NAME` for any scalar.

## Conventions

- `cargo build` must succeed with zero warnings at all times
- `cargo test` must pass before any task is considered complete
- After implementing a plan, run a short smoke (`--batch 1 --batch-max-turns 20`) to confirm the full game loop still works; reserve `--batch 3` (no turn cap) for changes whose effects only show up late-game
- `cargo clippy` — zero lints allowed
- `cargo fmt --check` — enforced formatting
- Domain crate depends only on `std` + `mlua` (Lua VM) — plus the narrow exceptions `serde`, `ron`, and `serde_json` documented in [ADR-0006](docs/adr/0006-rust-lua-boundary.md#dependency-policy). The `tests/architecture.rs::domain_has_only_serde_dependency` fixture enforces the allowlist; if you need to add a dep, update the ADR and the test together.
- Application crate depends only on domain
- Frontend and infrastructure never leak into domain
- Lua scripts are sandboxed — no file I/O, no network, no OS calls from Lua
- **All AI and game-mechanics variables live in Lua** — tunables for AI behavior (thresholds, weights, personality knobs) and game mechanics (economic rates, combat modifiers, diplomatic thresholds, tech effects, balance parameters) belong in Lua scripts or data files loaded by Lua, not as Rust constants. Rust holds the engine (turn pipeline, hex math, pathfinding, resolution); Lua holds the numbers. If you find a magic number in Rust that controls game feel or AI choices, move it to Lua.
- Every checklist item in `plan/` has a verification strategy runnable from the command line
- **No backward compatibility**: Old saves are not supported. Do not write migration code, save-version fallback paths, or compatibility shims. If you encounter existing backward-compat code, remove it.
- **Resource/material/goods names in the UI must include the emoji from `web/src/resourceEmoji.ts`** — use `resourceLabel(name)` for inline text like `"🌾 Grain"`, or `resourceEmoji(name)` for the symbol alone. Do not hardcode emoji or maintain a second mapping in a component. If a kind is missing from `resourceEmoji.ts`, add it there.

## Known Bugs

- **Non-determinism in turn processing**: Two games with the same map key produce different
  treasury values after ~20 turns (e.g., "Devron" gets $357,500 in one run vs $482,500 in another).
  Likely caused by HashMap iteration order in the turn processor or AI decision logic.
  The `test_determinism` test in `tests/simulation.rs` now checks dynamic state (treasury,
  army size, province ownership) and will fail until this is fixed. Root cause is in
  `crates/domain/src/turn/processor.rs` — look for HashMap iteration that feeds into
  order-dependent logic (resource distribution, AI decisions, combat resolution).

## Plan Index

All implementation checklists live in `plan/`:

| # | File | Area |
|---|------|------|
| 01 | [Architecture & Tech Stack](./plan/01-architecture.md) | Foundation |
| 02 | [Project Scaffolding](./plan/02-scaffolding.md) | Foundation |
| 03 | [Core Domain Model](./plan/03-core-domain.md) | Domain |
| 04 | [Map & Terrain](./plan/04-map-terrain.md) | Domain |
| 05 | [Resource & Production](./plan/05-resources-production.md) | Domain |
| 06 | [Town & Infrastructure](./plan/06-town-infrastructure.md) | Domain |
| 07 | [Transport System](./plan/07-transport.md) | Domain |
| 08 | [Technology Tree](./plan/08-technology.md) | Domain |
| 09 | [Diplomacy & Treaties](./plan/09-diplomacy.md) | Domain |
| 10 | [Trade & Economy](./plan/10-trade-economy.md) | Domain |
| 11 | [Military — Land Units](./plan/11-military-land.md) | Domain |
| 12 | [Military — Naval Units](./plan/12-military-naval.md) | Domain |
| 13 | [Combat System](./plan/13-combat.md) | Domain |
| 14 | [Nations & Scenarios](./plan/14-nations-scenarios.md) | Domain |
| 15 | [Turn Engine & Game Loop](./plan/15-turn-engine.md) | Domain |
| 16 | [AI Players](./plan/16-ai.md) | Domain |
| 17 | [Victory & Scoring](./plan/17-victory-scoring.md) | Domain |
| 18 | ~~UI — Main Screens~~ → merged into 29 | Presentation |
| 19 | [UI — Tactical Battle](./plan/19-ui-battle.md) | Presentation |
| 20 | [Audio & Music](./plan/20-audio.md) | Presentation |
| 21 | [Save / Load & Serialization](./plan/21-persistence.md) | Infrastructure |
| 22 | [Multiplayer & Networking](./plan/22-multiplayer.md) | Infrastructure |
| 23 | [Modding & Data-Driven Design](./plan/23-modding.md) | Infrastructure |
| 24 | [Testing Strategy](./plan/24-testing.md) | Quality |
| 25 | ~~Accessibility & Localization~~ → merged into 29 | Quality |
| 26 | [Performance & Optimization](./plan/26-performance.md) | Quality |
| 27 | [Build, CI/CD & Release](./plan/27-build-release.md) | Delivery |
| 28 | [Documentation](./plan/28-documentation.md) | Delivery |
| 29 | [Web Frontend: UI, Accessibility & Localization](./plan/29-web-frontend.md) | Presentation |

## Project Management

- **Trello board**: [Imperialism Remake](https://trello.com/b/WNTZdorA/imperialism-remake) (board id `69e37a93adccb0352eda9d18`). All follow-up cards, backlog items, and cross-cutting tickets belong here — never create cards on a different board. Lists include: AI, UI, Warfare, Economy & Production, Diplomacy, Performance, UI + Mechanics, General, Flavor, Later ideas, Done To Verify.
- **Moving Trello cards:** When you finish implementing a card, move it to the "Done to verify" list yourself. Leave a brief comment on the card referencing the commit, test, or verification step so the user knows what to check.

## Workflow

- After every implementation, run `/adversarial-review` before considering the task complete
- **When the work is ready for the user to test in the browser**, run `./web/restart-web-server.sh --opt` to rebuild WASM (optimized) and restart the dev server, then tell the user it's up at http://localhost:43173. Do this proactively at the end of any change that touches Rust code reachable from the WASM bridge (domain, application, infrastructure, wasm-bridge, or AI/game-config Lua) — don't wait for the user to ask.
- After committing and pushing, run `./web/restart-web-server.sh` to rebuild WASM and restart the dev server
