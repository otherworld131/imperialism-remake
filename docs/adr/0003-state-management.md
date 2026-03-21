# ADR-003: State Management

## Status
Accepted

## Context
Game state can be managed as immutable snapshots or mutable state with events.

## Decision
Mutable GameState aggregate root with domain events for notifications.
State serialized via serde for save/load.

## Consequences
- Simple mutation model, familiar to game developers
- Domain events enable loose coupling between systems
- Determinism requires care with HashMap iteration order
