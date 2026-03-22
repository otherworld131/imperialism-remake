# L5. `build ship` and `build warship` commands unreachable

**Severity:** Low — FIXED

**Root cause:** In the CLI match statement, the generic `build ` catch-all handler
(matching any command starting with "build ") came before the specific
`build ship <type>` and `build warship <type>` handlers. Since Rust match
selects the first matching arm, ship/warship build commands were caught by
the generic handler and reported as "Unknown building".

**Fix:**
- [x] Moved `build ship` and `build warship` match arms before the generic `build` catch-all
- [x] Removed duplicate match arms that were unreachable
- Commands now correctly report material requirements or build the ship
