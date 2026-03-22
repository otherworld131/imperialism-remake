# M8. Recently conquered provinces stay Hamlet — connectivity not updated for new owner

**Severity:** Moderate — RESOLVED (working as designed)

**Symptoms:** When a GP conquers provinces from another GP or minor, the conquered
provinces stay as Hamlet even though they should start industrializing under the
new owner's infrastructure network.

**Root cause:** `update_province_connectivity` only iterates over provinces owned by
each GP. After conquest, the province is in the new owner's `province_ids` but its
`connected_to_capital` flag may be `false` because the connectivity check runs based
on the new owner's capital, which may be far away from the conquered province.

The adjacent-tile connectivity check only works for provinces geographically close
to the owner's capital. Distant conquered provinces will never connect without
railroad infrastructure being built to them.

**Fix:** This is actually working as designed — provinces need infrastructure
(depots/railroads) to connect to the capital. The observation that some stay as
Hamlet is correct behavior for distant provinces. Added a note that this matches
the original Imperialism mechanics where conquered territory required investment.
