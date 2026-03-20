# Imperialism Remake

A faithful remake of **Imperialism** (Frog City Software, 1997) — a turn-based grand-strategy game set in the 19th-century Industrial Revolution.

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable, 1.85+)

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Quick Start

```bash
# Build
cargo build

# Run with default map
cargo run

# Run with a specific map key (deterministic — same key = same world)
cargo run -- "rome"

# Choose which Great Power to play (0-6)
cargo run -- "imperialism" 3
```

## Nation Index

| Index | Nation  | Color      |
|-------|---------|------------|
| 0     | Deneb   | Yellow     |
| 1     | Devron  | Orange     |
| 2     | Haxaco  | Light Blue |
| 3     | Kem     | Red        |
| 4     | Ordune  | Green      |
| 5     | Patagon | Purple     |
| 6     | Zimm    | Blue       |

## Running Tests

```bash
# All tests (200+)
cargo test --workspace

# Domain tests only
cargo test -p domain

# Linting
cargo clippy
cargo fmt --check
```

## Project Structure

```
crates/
├── domain/          Pure game logic (zero external deps)
├── application/     Use-cases, commands, queries
├── infrastructure/  Persistence, networking
└── presentation/    UI, rendering, audio
src/main.rs          CLI binary entry point
plan/                Implementation checklists (28 files)
```

See [CLAUDE.md](./CLAUDE.md) for architecture details and the full implementation plan.
