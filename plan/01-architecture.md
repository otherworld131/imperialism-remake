# 01 — Architecture & Tech Stack

## Architectural Style: Hexagonal (Ports & Adapters) + Clean Architecture

### Frontend / Backend Boundary

The backend is a **library crate** (`domain` + `application`). The frontend is a swappable
consumer. They communicate only through typed commands, queries, and view-model structs.

```
┌─────────────────────────────────────────────────────────────────┐
│                     FRONTEND (Presentation)                     │
│  Bevy / Godot GDExt / macroquad / WASM+canvas / headless CLI   │
│  Rendering, UI widgets, input, audio                            │
│  → sends Commands, receives ViewModels                          │
├─────────────────────────────────────────────────────────────────┤
│                  APPLICATION (Use-Cases)                        │
│  Commands: PlaceUnit, EndTurn, ProposeTreaty, SetTradeOffer     │
│  Queries:  GetMapView, GetNationSummary, GetTradePartners       │
│  Orchestrates domain, returns view models — never raw entities  │
├─────────────────────────────────────────────────────────────────┤
│                   BACKEND (Domain Core)                         │
│  Rust: hex math, combat, economy, turn resolution, pathfinding  │
│  Lua (mlua): tech tree logic, AI scripts, mod hooks, scenarios  │
│  ZERO knowledge of rendering, I/O, or platform                  │
├─────────────────────────────────────────────────────────────────┤
│                    INFRASTRUCTURE (Adapters)                    │
│  Save/Load (serde + bincode/JSON), Networking (tokio/quinn),    │
│  File system, Lua script loader, Platform services              │
│  Implements traits defined by Domain/Application                │
└─────────────────────────────────────────────────────────────────┘
```

### The backend as a library

Because the domain is a pure library crate, it can be consumed by:

- A **Bevy desktop game** (primary target)
- A **Godot frontend** via GDExtension FFI
- A **headless test harness** (AI-only simulations, CI)
- A **WASM module** behind a browser UI
- A **multiplayer server** (turn resolution only, no rendering)

This is the key architectural invariant. If the domain crate compiles and tests pass
with `--no-default-features` and no rendering framework, the boundary is intact.

## Checklist

### Language & Runtime — Rust
- [ ] Lock Rust edition and MSRV in `rust-toolchain.toml` (edition 2024, stable channel)
- [ ] Configure `Cargo.toml` workspace with all crates
- [ ] Enforce `#![deny(warnings, clippy::all)]` in all crates

### Scripting Layer — Lua
- [ ] Integrate `mlua` crate (Lua 5.4) in the domain crate
- [ ] Define Lua sandbox policy: no `os`, no `io`, no `loadfile` — only pure computation + game API
- [ ] Expose Rust game API to Lua: query game state, register callbacks, return decisions
- [ ] Lua script hot-reload during development (watch `scripts/` directory, reload on change)
- [ ] Lua error handling: script errors are caught and logged, never crash the engine
- [ ] Define script entry points:
  - [ ] `scripts/tech/*.lua` — tech effect definitions, prerequisite logic
  - [ ] `scripts/ai/*.lua` — AI decision functions per personality (aggressive, diplomatic, economic, balanced)
  - [ ] `scripts/scenarios/*.lua` — scenario event triggers, custom victory conditions
  - [ ] `scripts/mods/*.lua` — mod hook entry points (on_turn_start, on_combat_end, on_trade_resolved, …)
- [ ] Unit test: load a Lua script → call a function → verify return value
- [ ] Unit test: Lua sandbox blocks `os.execute()`, `io.open()`, `require()`
- [ ] Unit test: Lua error in script → engine continues, error logged

### What is Rust vs. What is Lua

| Concern | Language | Rationale |
|---------|----------|-----------|
| Hex math, coordinates | Rust | Performance-critical, called millions of times |
| Combat resolution engine | Rust | Deterministic, hot-path, must be identical in multiplayer |
| Turn resolution pipeline | Rust | Orchestration, ordering guarantees |
| Production chain math | Rust | Simple arithmetic, no flexibility needed |
| Pathfinding (A*, BFS) | Rust | Performance-critical |
| Serialization (save/load) | Rust (serde) | Binary format, versioning, no script involvement |
| Tech tree definitions | Lua | Modders add techs, change effects, rebalance costs |
| Tech effect application | Lua | "When researched, unlock unit X, enable improvement Y" |
| AI high-level strategy | Lua | Swappable personalities, difficulty tuning, moddable |
| AI tactical decisions | Rust | Performance (many units, many options per combat turn) |
| Scenario events/triggers | Lua | "On turn 40, if nation X controls province Y, trigger Z" |
| Mod hooks | Lua | on_turn_start, on_combat_end, on_province_conquered, … |
| Balance parameters | RON/JSON + Lua | Static data in RON; dynamic overrides in Lua |
| UI rendering | Rust (Bevy) | Framework-native, performance |
| Audio | Rust (Bevy) | Framework-native |

### Game Framework (Frontend)
- [ ] Evaluate **Bevy** — pure Rust ECS, active ecosystem, cross-platform, WASM support
- [ ] Evaluate **Godot 4 via GDExtension** — mature editor, hex support, scene system, visual scripting
- [ ] Evaluate **macroquad** — minimal, immediate-mode, fast to prototype
- [ ] Decision: framework chosen and documented in ADR-001
- [ ] Prototype: render a hex grid, handle input, play a sound — validate the choice
- [ ] Verify frontend is fully swappable: domain crate compiles + tests pass without any framework dependency

### Dependency Rule Enforcement
- [ ] Domain crate `Cargo.toml`: only `mlua` (+ `serde` with `derive` feature for data structs)
- [ ] Application crate: depends only on domain
- [ ] Infrastructure: depends on application + external crates (serde, bincode, tokio, etc.)
- [ ] Presentation: depends on application + framework crate (bevy, godot, etc.)
- [ ] Host binary (`main.rs`): depends on all — this is the composition root
- [ ] Enforce via crate boundaries + `cargo test --test architecture` that parses `Cargo.toml` dependency graphs
- [ ] Write architectural fitness function tests that fail on illegal dependencies
- [ ] `domain` crate compiles with `cargo build -p domain` in isolation

### Cross-Platform Abstractions (Traits)
- [ ] `PlatformServices` trait — file paths, clipboard, notifications
- [ ] `RenderSurface` trait — thin abstraction over the framework's draw calls
- [ ] `AudioEngine` trait — play BGM, SFX, set volume
- [ ] `InputProvider` trait — keyboard, mouse, gamepad abstraction
- [ ] `ScriptLoader` trait — load Lua scripts from file system or embedded resources
- [ ] Windows adapter implementations
- [ ] macOS adapter stubs (future)
- [ ] Linux adapter stubs (future)
- [ ] WASM adapter stubs (future)

### Event Bus / Mediator
- [ ] In-process event bus for domain events (`TurnEnded`, `WarDeclared`, `TechResearched`, …)
- [ ] Events are immutable Rust structs
- [ ] Handlers registered at composition root, executed synchronously within the game loop
- [ ] Lua scripts can register event listeners (e.g., mod hook: `on("TurnEnded", function(turn) ... end)`)
- [ ] Unit test: publish event → Rust handler fires → side-effect observable
- [ ] Unit test: publish event → Lua listener fires → side-effect observable

### Dependency Injection (Manual)
- [ ] Composition root in `main.rs` — constructs all objects, wires all trait implementations
- [ ] All ports defined as traits; adapters as concrete `struct`s implementing those traits
- [ ] Trait objects (`Box<dyn Trait>`) for runtime polymorphism where needed
- [ ] Generics for compile-time polymorphism in hot paths
- [ ] No DI framework — Rust's type system + manual wiring is sufficient

### Architectural Decision Records (ADRs)
- [ ] ADR-001: Language & framework selection (Rust + Lua + Bevy/Godot)
- [ ] ADR-002: Hex coordinate system (axial vs. offset vs. cube)
- [ ] ADR-003: State management approach (immutable snapshots vs. mutable + events)
- [ ] ADR-004: AI architecture (Rust engine + Lua strategy scripts)
- [ ] ADR-005: Networking model (lockstep vs. client-server vs. relay)
- [ ] ADR-006: Rust/Lua boundary — what lives where and why
- [ ] ADR-007: Frontend framework selection and swappability guarantees
- [ ] Template: `docs/adr/NNNN-title.md` with Status / Context / Decision / Consequences

### Verification Strategy
- [ ] **Compile check**: `cargo build` succeeds with zero warnings across all crates
- [ ] **Domain isolation test**: `cargo build -p domain` compiles with no framework dependency
- [ ] **Dependency rule test**: `cargo test --test architecture` → verify domain has only allowed deps, application refs only domain, etc.
- [ ] **Lua sandbox test**: `cargo test -p domain -- lua_sandbox` → verify blocked APIs raise errors
- [ ] **Lua integration test**: Load `scripts/tech/test_tech.lua` → call `get_tech_effect("seed_drill")` → verify correct return
- [ ] **Port/Adapter test**: Instantiate each adapter → verify it implements its port trait correctly
- [ ] **Frontend swap test**: Build domain + application without presentation crate → compiles and tests pass
- [ ] **Prototype validation**: Hex grid renders, click detected, sound plays — captured in a smoke test script
- [ ] **Cross-platform check**: `cargo build --target wasm32-unknown-unknown -p domain` compiles (WASM viability)
- [ ] **ADR review**: Every ADR has Status, Context, Decision, Consequences sections
