# Imperialism Remake

A faithful remake of **Imperialism** (Frog City Software, 1997) — a turn-based grand-strategy game set in the 19th-century Industrial Revolution.

Lead one of seven Great Powers competing for world dominance through economics, diplomacy, and military conquest. Research technologies, build infrastructure, trade with Minor Nations, raise armies, and win the Council of Governors.

## Quick Start

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build and run
cargo run

# Play a historical scenario
cargo run -- --scenario 1848

# Play with a specific map key and nation
cargo run -- "rome" 3
```

## Historical Scenarios

| Scenario | Year | Description |
|----------|------|-------------|
| `--scenario 1815` | 1815 | Congress of Vienna — post-Napoleonic Europe |
| `--scenario 1820` | 1820 | Concert of Europe — fragile balance of power |
| `--scenario 1848` | 1848 | Year of Revolutions — empires tremble |
| `--scenario 1882` | 1882 | Scramble for Africa — colonial dominance |

## Commands

| Category | Commands |
|----------|----------|
| **Economy** | `warehouse`, `buildings`, `pop`, `transport`, `trade`, `fleet`, `build`, `expand`, `recruit`, `train`, `sell`, `build car`, `build ship`, `produce arms` |
| **Military** | `army`, `build unit`, `move`, `attack`, `upgrade`, `navy`, `build warship`, `blockade` |
| **Diplomacy** | `diplomacy`, `consulate`, `embassy`, `war`, `peace`, `pact`, `alliance`, `grant`, `subsidy` |
| **Civilians** | `civilians`, `hire`, `deploy` |
| **Technology** | `tech`, `research` |
| **Infrastructure** | `build railroad`, `build depot`, `build port`, `build fort`, `infra` |
| **Map** | `map`, `provinces`, `info`, `nations`, `score`, `overview`, `history` |
| **Game** | `turn`, `auto`, `save`, `load`, `quicksave`, `quickload`, `orders`, `scenarios`, `help`, `quit` |

## Nation Index (Random Maps)

| Index | Nation  | Color      | AI Personality |
|-------|---------|------------|----------------|
| 0     | Deneb   | Yellow     | Balanced       |
| 1     | Devron  | Orange     | Aggressive     |
| 2     | Haxaco  | Light Blue | Economic       |
| 3     | Kem     | Red        | Aggressive     |
| 4     | Ordune  | Green      | Diplomatic     |
| 5     | Patagon | Purple     | Economic       |
| 6     | Zimm    | Blue       | Balanced       |

## Game Systems

- **Economy**: 3 production chains (Timber→Lumber→Furniture, Coal+Iron→Steel→Hardware, Cotton/Wool→Fabric→Clothing), town autonomy (Hamlet→Village→Town), food/starvation, immigration
- **Military**: 22 army unit types, 13 ship types, medals (4=2× firepower), upgrades, Generals, combat with terrain/fort bonuses
- **Diplomacy**: Consulates, embassies, pacts, alliances, cash grants, trade subsidies, Council of Governors voting
- **Technology**: 28 technologies from the Industrial Revolution era
- **Trade**: Resource trading with 16 Minor Nations, supply/demand pricing, blockades
- **AI**: 4 distinct personalities (Aggressive/Diplomatic/Economic/Balanced), difficulty bonuses

## Running Tests

```bash
cargo test --workspace    # 982 tests
cargo clippy              # Zero warnings
cargo fmt --check         # Enforced formatting
./scripts/test.sh         # Full check suite
./scripts/smoke_test.sh   # Quick smoke test
```

## Project Stats

| Metric | Value |
|--------|-------|
| Lines of Rust | 36,900+ |
| Tests | 982 |
| Source files | 51 |
| Release binary | 2.3 MB |
| Full game (400 turns) | ~7 seconds |
| Turn resolution | ~13 ms |

## Architecture

Hexagonal (Ports & Adapters) architecture with a hard frontend/backend boundary:

```
crates/
├── domain/          Pure game logic (serde only dependency)
├── application/     Screen queries, view models
├── infrastructure/  Save/load (JSON + versioning)
└── presentation/    (Future: Bevy/Godot graphical frontend)
src/main.rs          CLI binary (current frontend)
data/                RON definitions, localization, scenarios
docs/adr/            7 Architectural Decision Records
plan/                28 implementation checklists (1,058/1,633 items complete)
```

See [CLAUDE.md](./CLAUDE.md) for full architecture documentation.

## License

MIT
