# ADR-006: Rust/Lua Boundary

## Status
Accepted (revised 2026-05-09 to record the WASM build-time bake)

## Decision
Rust handles: hex math, combat, turn resolution, pathfinding, serialization.
Lua handles: tech effects, AI strategies, scenario scripting, mod hooks, all
gameplay/AI tunables (thresholds, weights, balance numbers).
Boundary: Lua runs sandboxed (no I/O) with the game API exposed by Rust.

## Consequences
- Engine performance where it matters
- Modder-friendly scripting where flexibility matters
- A Lua script edit (no Rust rebuild required for tunable changes on native
  builds, where the Lua VM reads scripts at startup)

## WASM split: Lua values are baked at build time

The Lua VM (`mlua`) is not embedded in the WASM binary — including it would
roughly triple the wasm payload and pull in a JIT that browsers cannot run.
WASM builds therefore compile with the `lua` feature turned off
(`crates/domain/Cargo.toml`).

To keep the browser and the CLI on the same numbers, `crates/domain/build.rs`
runs `mlua` natively at compile time, evaluates every script under
`scripts/`, and emits `$OUT_DIR/lua_baked.json` containing the parsed
`game_config` and per-personality `LuaAiConfig` tables. The non-Lua build of
the domain crate embeds that JSON via `include_str!` and decodes it on first
access in `crates/domain/src/ai/lua_bridge.rs::baked::parse_baked`.

```
scripts/*.lua  ──build.rs──>  $OUT_DIR/lua_baked.json  ──include_str!──>  domain (WASM)
                                                                          │
                                                                          serde_json::from_str on first use
```

Native builds keep the live Lua VM and read the same tables directly — the
two paths produce the same `GameConfig` / `LuaAiConfig` values by
construction. `LuaAiConfig::sanitize()` is applied on both paths so
out-of-range or NaN values in the JSON behave identically to the Lua loader.

## Dependency Policy

The headline rule from `CLAUDE.md` is "domain crate depends only on `std` +
`mlua`". The actual allowlist enforced by
`tests/architecture.rs::domain_has_only_serde_dependency` is:

| Dep | Why it lives in domain |
|-----|------------------------|
| `mlua` | Optional (`feature = "lua"`). Native-only; absent on WASM. |
| `serde` | The baked-Lua tables (`GameConfig`, `LuaAiConfig`) need `Deserialize` so the WASM build can decode them; serde derives are also used on a handful of value types that cross the snapshot/wire boundary. |
| `ron` | Test fixtures and a few embedded data files that live under `data/definitions/`. |
| `serde_json` | Required by the WASM-side baked-Lua decoder (`include_str!(...)` + `serde_json::from_str`). The build script writes JSON because it's the lowest-friction format that round-trips through `serde::Deserialize` without a custom parser. |

`serde_json` is the most ear-catching one. It's listed because:

- The architectural ideal is that JSON encoding/decoding is an
  *infrastructure* concern owned by `wasm-bridge` and `domain-snapshot`,
  with the domain knowing only about typed data.
- The Lua-baking pipeline forces a one-time exception: the build script runs
  before any non-domain crate could intervene, and the simplest way to ship
  the baked values into the no-Lua domain is "embed JSON, decode at
  startup". Inventing a bespoke binary format would buy nothing and
  re-implementing serde-derive for the few targeted types would be churn
  without payoff.
- Acceptable alternatives, if the leak ever needs to be closed:
  1. **Generate Rust source from `build.rs`**: emit
     `pub static BAKED_LUA: &LuaBakedDocument = &LuaBakedDocument { ... };`
     instead of JSON. Removes the runtime decode and the `serde_json` dep,
     at the cost of a more complex code generator.
  2. **Move the decode out of domain**: have the WASM bridge or a dedicated
     adapter crate own the JSON-shaped baked tables and inject the decoded
     `GameConfig` into domain at startup. Preserves architectural purity
     but adds a layer of plumbing that today buys nothing user-visible.

If you need to add another dependency to the domain crate, update both the
allowlist in `tests/architecture.rs:94` and this ADR. Don't extend the
allowlist silently — every entry should have a paragraph here explaining
why the architecture rule had to bend.
