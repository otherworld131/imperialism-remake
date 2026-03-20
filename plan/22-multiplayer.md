# 22 — Multiplayer & Networking

## Overview

The original supported up to 7 players over LAN or Internet. The remake should support
modern networking. Architecture accounts for multiplayer from day one even though
single-player is the priority.

## Checklist

### Networking Architecture
- [ ] Choose model: **Client-Server** recommended (one player hosts, others connect)
- [ ] Alternative: relay server for NAT traversal
- [ ] Document decision in ADR-005
- [ ] `NetworkAdapter` trait — domain never touches network directly
- [ ] Server-authoritative: host runs the turn engine; clients submit orders and receive results
- [ ] Deterministic turn resolution ensures all clients stay in sync
- [ ] Unit tests: network adapter trait compliance

### Protocol Design
- [ ] Define message types: JoinGame, LeaveGame, SubmitOrders, TurnResult, ChatMessage, SyncState
- [ ] Binary serialization for messages (low overhead)
- [ ] Message framing and length prefixing
- [ ] Heartbeat/keepalive for connection monitoring
- [ ] Reconnection support: client can rejoin mid-game with state sync
- [ ] Unit tests: message serialization roundtrip

### Lobby System
- [ ] Host creates game (selects map, difficulty, max players)
- [ ] Clients discover host via LAN broadcast or direct IP/code entry
- [ ] Nation selection: each player picks a Great Power (no duplicates)
- [ ] Ready-up system: all players must confirm before game starts
- [ ] Chat functionality in lobby
- [ ] Host can kick players, adjust settings
- [ ] Unit tests: lobby state machine (waiting → ready → started)

### Turn Synchronization
- [ ] All players must submit orders before turn resolves
- [ ] Timer option: auto-submit after N minutes (configurable, e.g., 5 min)
- [ ] "Waiting for players" indicator shows who hasn't submitted
- [ ] Turn resolution runs on host; results broadcast to all clients
- [ ] Clients validate received state against their local expectations
- [ ] Desync detection: hash game state each turn, compare across clients
- [ ] Unit tests: sync protocol correctness

### Multiplayer-Specific Features
- [ ] Simultaneous turns (all players submit independently)
- [ ] Fog of diplomacy: players can't see each other's exact scores/relations (only estimates)
- [ ] Private diplomacy: treaty proposals visible only to involved parties
- [ ] In-game chat: all-chat and private messaging
- [ ] Pause/resume: host can pause the game (with vote option)
- [ ] Save/load multiplayer games (all clients must agree to load)
- [ ] Dropout handling: disconnected player replaced by AI until reconnect
- [ ] Unit tests: dropout → AI takeover → reconnect → player resumes

### Security
- [ ] Order validation on server (prevent cheating via invalid orders)
- [ ] No client access to hidden information (mineral deposits, AI plans)
- [ ] Rate limiting to prevent spam
- [ ] Authentication: simple session tokens (no accounts needed for LAN)
- [ ] Unit tests: server rejects invalid/impossible orders

### Performance
- [ ] Network message size optimization (delta updates where possible)
- [ ] Latency tolerance: game should function with up to 500ms round-trip
- [ ] Bandwidth estimation: < 50KB per turn per player
- [ ] Connection quality indicator in UI

### Verification Strategy
- [ ] **Unit tests**: All networking tests pass
- [ ] **Integration test**: 2 clients connect to host → both submit orders → turn resolves → both receive identical results
- [ ] **Sync test**: Run 50 turns with 3 players → verify game state hash matches on all clients every turn
- [ ] **Dropout test**: Player disconnects mid-game → AI takes over → player reconnects → resumes correctly
- [ ] **Stress test**: 7 players, 100 turns → no desyncs, no crashes
- [ ] **Latency test**: Simulate 200ms latency → verify game remains playable and orders resolve correctly
- [ ] **Security test**: Submit malformed/impossible orders → verify server rejects them gracefully
