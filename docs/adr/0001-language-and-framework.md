# ADR-001: Language & Framework Selection

## Status
Accepted

## Context
Need a language that provides: cross-platform support (Windows/macOS/Linux/WASM),
high performance for turn resolution and AI, memory safety, and a mature game ecosystem.

## Decision
- **Primary language**: Rust (edition 2024, stable toolchain)
- **Scripting**: Lua (via mlua) for moddable game logic (deferred to post-MVP)
- **Frontend**: CLI for MVP; Bevy/Godot planned for graphical frontend
- **Serialization**: serde with JSON for save files

## Consequences
- Zero-cost abstractions and no GC pauses ensure fast turn resolution
- Strict type system catches bugs at compile time
- Steeper learning curve offset by long-term maintainability
- WASM target enables future browser deployment
