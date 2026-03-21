# ADR-005: Networking Model

## Status
Proposed (not yet implemented)

## Decision
Client-server model planned. Host runs turn engine, clients submit orders.
Deterministic turn resolution ensures sync.

## Consequences
- Server-authoritative prevents cheating
- Reconnection support via state sync
- Deferred to post-MVP
