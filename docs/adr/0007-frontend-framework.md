# ADR-007: Frontend Framework

## Status
Proposed (CLI for MVP)

## Decision
Domain is a library crate with zero rendering dependencies.
CLI binary for MVP. Bevy or Godot GDExtension for graphical frontend.

## Consequences
- Domain crate compilable without any framework
- Frontend fully swappable
- CLI enables rapid development and testing
