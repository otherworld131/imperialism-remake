# ADR-004: AI Architecture

## Status
Accepted

## Context
AI needs to manage economy, diplomacy, military, and trade for 6 Great Powers + 16 Minor Nations.

## Decision
Rust-based AI with personality system (Aggressive, Diplomatic, Economic, Balanced).
Utility scoring for tactical decisions, rule-based for strategic decisions.
Lua scripting for AI behavior deferred to post-MVP.

## Consequences
- Fast AI processing (< 1 second for all nations)
- Four distinct AI personalities create varied gameplay
- Deterministic assignment per nation index
