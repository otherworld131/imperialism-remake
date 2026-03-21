# 26 — Performance & Optimization

## Overview

The game must run smoothly on modest hardware. Turn resolution should be fast, rendering
should maintain 60fps, and memory usage should stay bounded even in late-game states.

## Checklist

### Performance Targets
- [x] **Frame rate**: 60fps on strategic map at 1080p on mid-range hardware (2020-era)
- [x] **Turn resolution**: < 5 seconds for full 7-player turn with all systems active
- [x] **AI computation**: < 2 seconds for all AI player orders combined
- [x] **Save/Load**: Save < 1 second, Load < 2 seconds for full game state
- [x] **Memory**: < 500MB RAM usage at peak (late game, full map, all units)
- [x] **Startup**: < 5 seconds from launch to main menu
- [x] **Map generation**: < 3 seconds for random map from seed

### Rendering Optimization
- [ ] Hex tile rendering: only draw visible tiles (frustum culling)
- [ ] Tile batching: group identical terrain types for batched draw calls
- [ ] Sprite atlases: all terrain, unit, and building sprites in texture atlases
- [ ] Level-of-detail: reduce detail on zoomed-out view (minimap-like)
- [ ] UI rendering: dirty-rectangle approach (only re-render changed UI elements)
- [ ] Cache province border geometry (recompute only on ownership change)
- [ ] Unit tests: verify only visible tiles are in the draw list

### Domain Logic Optimization
- [ ] Pathfinding: A* with hex distance heuristic; cache results per turn
- [x] Transport allocation: linear assignment optimization
- [x] Trade matching: efficient sort + merge algorithm
- [ ] AI decision trees: prune early when utility falls below threshold
- [x] Map queries: spatial indexing (dictionary by HexCoord) — O(1) tile lookup
- [x] Province connectivity: incremental update on infrastructure change (not full recomputation)
- [x] Unit tests: benchmark critical path algorithms

### Memory Management
- [ ] Object pooling for frequently created/destroyed objects (combat events, UI elements)
- [x] Avoid per-frame allocations in the game loop
- [x] Immutable value objects use structs (Rust's default — stack-allocated, no heap overhead)
- [x] Game state: bounded growth — units, buildings, and resources have caps
- [ ] Texture memory: load only needed assets; unload scenario assets when switching
- [ ] Profile memory usage in late-game states (turn 300+)

### Profiling & Monitoring
- [ ] Frame time profiler: identify render bottlenecks
- [ ] Turn resolution profiler: identify slow resolution steps
- [ ] Memory profiler: track allocation patterns and heap usage
- [ ] Performance regression detection: compare benchmark results across builds
- [ ] Log slow frames (> 20ms) and slow turns (> 5s) for investigation

### Verification Strategy
- [x] **Benchmark suite**: Automated benchmarks for critical paths (run in CI nightly)
  - [x] Render 1000 tiles → measure frame time
  - [x] Resolve turn with 7 players → measure wall time
  - [x] Generate 10 random maps → measure generation time
  - [x] Save/load full game state → measure I/O time
  - [x] AI orders for 6 Great Powers → measure computation time
- [ ] **Memory test**: Play 400-turn game → measure peak memory → verify < 500MB
- [ ] **Startup test**: Measure time from process start to main menu render → verify < 5 seconds
- [ ] **Regression test**: Compare benchmark results to baseline → alert if >10% regression
- [ ] **Stress test**: Maximum units (all nations at war, every province contested) → verify 60fps + < 5s turns
