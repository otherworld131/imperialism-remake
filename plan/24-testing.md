# 24 — Testing Strategy

## Overview

A superlative product demands a superlative testing strategy. Tests span from unit-level
domain logic to full game simulations. The test suite must be fast, reliable, and
comprehensive.

## Checklist

### Test Pyramid

#### Layer 1 — Unit Tests (Domain Core)
- [x] Every value object: equality, immutability, arithmetic, edge cases
- [x] Every entity: creation, state transitions, invariant enforcement
- [x] Every domain service: correct output for known inputs
- [x] Hex coordinate math: 30+ test cases covering all operations
- [x] Production chain calculations: all combinations of inputs → outputs
- [x] Combat resolution: deterministic outcomes for fixed inputs
- [x] Tech tree: prerequisite validation, availability windows, effects
- [x] Diplomacy: relationship score calculations, treaty rules
- [x] AI decision-making: valid orders for known game states
- [x] Victory/scoring: correct calculations from known state
- [ ] Target: **≥ 90% code coverage** on Domain crate
- [x] All domain tests run without I/O, network, or framework dependencies
- [x] Execution time: < 30 seconds for entire domain test suite

#### Layer 2 — Application Tests (Use Cases)
- [x] Command handlers: valid commands produce correct state changes
- [x] Query handlers: return correct data for known states
- [x] Validation: invalid commands rejected with clear errors
- [x] Event handling: domain events trigger correct side effects
- [x] Target: **≥ 85% coverage** on Application crate
- [x] Execution time: < 15 seconds

#### Layer 3 — Integration Tests
- [x] Save/Load roundtrip: save state → load → compare
- [x] Full turn resolution: 7 nations, all systems active, no errors
- [ ] Multiplayer sync: 2+ clients receive identical results
- [x] Data loading: all definition files load and validate correctly
- [ ] Mod system: mods apply correctly, conflicts detected
- [x] Execution time: < 60 seconds

#### Layer 4 — Simulation Tests (Game-Level)
- [x] 10-turn smoke test: game progresses without errors
- [x] 100-turn endurance test: no state corruption, memory leaks, or runaway values
- [x] Full game test (to 1915): game completes with valid winner
- [x] AI-only games: 100 games → verify win distribution, no crashes
- [x] Balance tests: no single nation/strategy dominates excessively
- [x] Performance baseline: turn resolution < 5s, frame time < 16ms
- [x] Execution time: < 10 minutes (can run in CI on schedule, not every commit)

#### Layer 5 — Architecture Tests (Fitness Functions)
- [x] Domain crate has zero external dependencies
- [x] Application crate references only Domain
- [x] No circular dependencies between crates
- [x] No infrastructure types referenced from Domain
- [x] All ports (traits) defined in Domain or Application
- [x] All adapters defined in Infrastructure or Presentation
- [x] Execution time: < 5 seconds

### Test Infrastructure
- [x] Test framework: built-in `#[test]` + `rstest` or `test-case`
- [x] Assertion library: `assert_eq!`, `assert_matches!`, `pretty_assertions` crate (readable assertions)
- [ ] Mocking library: `mockall` crate (for port/trait mocking)
- [x] Test data builders: fluent builders for complex domain objects
- [x] `GameStateBuilder` — creates game states for testing with sensible defaults
- [x] `NationBuilder`, `ProvinceBuilder`, `UnitBuilder`, etc.
- [x] Deterministic RNG: seedable random for reproducible tests
- [x] Test fixtures for common scenarios (early game, mid game, late game, war, peace)

### Property-Based Tests
- [x] Map generation: random seeds always produce valid maps
- [x] Hex math: coordinate conversions are invertible
- [x] Serialization: roundtrip always preserves equality
- [x] Production: output never exceeds input (conservation laws)
- [x] Combat: total damage dealt never exceeds total health available

### CI Verification Steps (How Claude Code Validates Each Step)
- [x] `cargo build` — project compiles with zero warnings
- [x] `cargo test --lib` — all unit tests pass
- [x] `cargo test --test integration` — all integration tests pass
- [x] `cargo test --test architecture` — all fitness functions pass
- [x] `cargo fmt --check` — code formatting is correct
- [x] `cargo clippy` — no linter warnings
- [ ] Test coverage report generated — verify coverage thresholds met
- [ ] Simulation tests run on schedule (nightly) — results reported

### Verification Strategy (Meta)
- [x] **Run tests after every code change**: `cargo test` must pass before considering a task complete
- [x] **Compile check**: `cargo build` must succeed with zero warnings
- [ ] **Coverage check**: Generate coverage report → verify thresholds
- [x] **Regression check**: No previously passing test starts failing
- [x] **Performance baseline**: Measure and record execution times; alert on significant regression
