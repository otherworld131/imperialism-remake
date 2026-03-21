# 02 — Project Scaffolding

## Workspace Layout

```
imperialism-remake/
├── CLAUDE.md
├── Cargo.toml                   # Workspace root
├── rust-toolchain.toml          # Rust edition + MSRV
├── crates/
│   ├── domain/                  # Pure game logic — deps: std + mlua only
│   │   ├── src/
│   │   │   ├── hex/             # Coordinates, spatial queries
│   │   │   ├── map/             # Terrain, provinces, sea zones
│   │   │   ├── economy/         # Resources, production, trade, transport
│   │   │   ├── military/        # Units, combat, naval
│   │   │   ├── diplomacy/       # Treaties, relations, council
│   │   │   ├── tech/            # Tech tree engine (Rust + Lua bridge)
│   │   │   ├── ai/              # AI framework (Rust engine + Lua strategies)
│   │   │   ├── turn/            # Turn processor, game loop
│   │   │   └── lib.rs
│   │   └── Cargo.toml
│   ├── application/             # Use-cases, commands, queries — deps: domain
│   ├── infrastructure/          # Persistence, networking — deps: application, serde, tokio
│   └── presentation/            # UI, rendering, audio — deps: application, bevy/godot
├── src/
│   └── main.rs                  # Composition root, binary entry point
├── scripts/                     # Lua game scripts
│   ├── tech/                    # Tech tree definitions & effects
│   ├── ai/                      # AI behavior per personality
│   ├── scenarios/               # Scenario event scripts
│   └── mods/                    # Mod hook entry points
├── tests/
│   ├── unit/                    # Per-crate unit tests (also in crate src/)
│   ├── integration/             # Cross-crate integration tests
│   ├── architecture/            # Dependency rule fitness functions
│   └── simulation/              # Multi-turn game simulations
├── data/
│   ├── definitions/             # RON/JSON: units, techs, buildings, nations
│   ├── scenarios/               # Scenario map + config files
│   ├── sprites/
│   ├── audio/
│   └── localization/
├── docs/
│   ├── adr/
│   ├── game-design/
│   └── api/
├── tools/
│   ├── map-editor/
│   └── scenario-editor/
└── build/
    ├── ci/
    └── scripts/
```

## Checklist

### Repository Setup
- [ ] Initialize git repository
- [ ] Create `.gitignore` (binaries, IDE files, build artifacts, OS files)
- [ ] Create `.editorconfig` (indentation, charset, line endings)
- [ ] Set up branch protection rules (`main` requires PR + CI green)
- [ ] Create `CONTRIBUTING.md` with coding standards
- [ ] Create `LICENSE` file

### Project Structure
- [x] Create `Cargo.toml` workspace with all member crates
- [x] Create `rust-toolchain.toml` — pin edition 2024, stable channel
- [x] Create `crates/domain` — library crate; deps: `mlua` (Lua 5.4, sandboxed)
- [x] Create `crates/application` — library crate; deps: `domain` only
- [x] Create `crates/infrastructure` — library crate; deps: `application`, `serde`, `bincode`, `tokio`
- [x] Create `crates/presentation` — library crate; deps: `application`, `bevy` (or chosen framework)
- [x] Create `src/main.rs` — binary entry point, composition root, depends on all crates
- [ ] Create `tests/architecture/` — fitness function tests parsing `Cargo.toml` dependency graphs
- [ ] Create `tests/integration/` — cross-crate integration tests
- [ ] Create `tests/simulation/` — multi-turn automated game tests

### Lua Scripts Directory
- [ ] Create `scripts/tech/` with a sample tech definition (`seed_drill.lua`)
- [ ] Create `scripts/ai/` with a stub AI personality (`balanced.lua`)
- [ ] Create `scripts/scenarios/` with a stub scenario trigger
- [ ] Create `scripts/mods/` with a documented hook template
- [ ] Verify Lua scripts load and execute from `cargo test -p domain`

### Data Directory
- [ ] Create `data/definitions/` with placeholder RON files (units.ron, ships.ron, terrain.ron, buildings.ron, nations.ron)
- [ ] Create `data/scenarios/` with a minimal test scenario
- [ ] Create `data/sprites/` with placeholder assets
- [ ] Create `data/audio/` directory structure (bgm/, sfx/)
- [ ] Create `data/localization/en.json` with initial string table

### Tooling
- [x] Code formatter configured and enforced (`cargo fmt`)
- [x] Linter configured (`cargo clippy`)
- [x] Pre-commit hook: format + lint
- [x] IDE run configurations for Debug, Release, Tests
- [x] Script: `build.sh` / `build.ps1` — clean build from scratch
- [x] Script: `test.sh` / `test.ps1` — run all test suites

### Initial Smoke Test
- [ ] Host boots and opens a window with the game framework
- [ ] A hex grid renders on screen (even with placeholder tiles)
- [ ] A click on a hex tile is detected and logged
- [ ] A sound effect plays on click
- [ ] Application shuts down cleanly with no resource leaks

### Verification Strategy
- [ ] **Build from scratch**: Clone repo → run `build.sh`/`build.ps1` → exits 0, all crates compile
- [ ] **Test runner**: `cargo test` finds and executes all test crates → 0 failures
- [ ] **Lint check**: `cargo fmt --check` → no formatting violations
- [ ] **Directory check**: Run `ls -R src/ tests/ data/` → verify expected directory structure exists
- [ ] **Smoke test script**: Automated script launches the app, waits 5 seconds, verifies process ran and exited cleanly
- [ ] **Pre-commit hook test**: Stage a badly formatted file → commit → hook rejects it
