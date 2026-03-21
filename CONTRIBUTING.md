# Contributing to Imperialism Remake

## Build & Test

```bash
cargo build          # compile all crates
cargo test --workspace  # run all tests (600+)
cargo clippy         # lint check (zero warnings required)
cargo fmt --check    # formatting check
```

## Architecture

See [CLAUDE.md](./CLAUDE.md) for full architecture documentation.

Key rules:
- Domain crate has minimal dependencies (only serde + std)
- Application crate depends only on domain
- Infrastructure and presentation never leak into domain
- All game logic in domain crate, UI/IO in presentation/infrastructure

## Code Style

- `cargo fmt` enforced
- `cargo clippy` zero warnings
- Tests required for all new domain logic
- Commits: `<area>: <description>` format

## ADRs

Architectural decisions documented in `docs/adr/`. Follow the template:
Status / Context / Decision / Consequences.
