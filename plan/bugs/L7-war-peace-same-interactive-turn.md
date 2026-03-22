# L7. Player can declare war and make peace in same interactive turn

**Severity:** Low / UX — ACKNOWLEDGED

**Symptoms:** In interactive mode, the player can type `war hurshen` followed by
`peace hurshen` in the same turn without any restriction. The game allows
contradictory diplomatic actions within a single turn.

**Note:** This is a UX edge case — the player is deliberately issuing contradictory
commands. The game correctly processes each command in sequence. Not exploitable
since the AI processes between turns.
