# ADR-006: Rust/Lua Boundary

## Status
Proposed (Lua not yet integrated)

## Decision
Rust handles: hex math, combat, turn resolution, pathfinding, serialization.
Lua handles: tech effects, AI strategies, scenario scripting, mod hooks.
Boundary: Lua runs sandboxed (no I/O) with game API exposed by Rust.

## Consequences
- Engine performance where it matters
- Modder-friendly scripting where flexibility matters
- Deferred to post-MVP (all logic currently in Rust)
