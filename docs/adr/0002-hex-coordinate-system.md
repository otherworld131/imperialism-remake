# ADR-002: Hex Coordinate System

## Status
Accepted

## Context
Hex grids can use offset, axial (cube), or doubled coordinates.

## Decision
Axial coordinates (q, r) with cube constraint q + r + s = 0.
Pointy-top orientation. Based on Red Blob Games hex grid reference.

## Consequences
- Simple arithmetic for distance, neighbors, rings, lines
- Clean pixel conversion for rendering
- Efficient storage (2 ints per coordinate)
