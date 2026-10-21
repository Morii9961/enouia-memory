# ADR-MEM-74 — Native Host binding for domain writes

Date: 2026-10-22. Scope: owner-authorized MV-8.1e native library.

Domain services accept `&Vault` and call `Vault::commit`; some replay receipts
or read duplicate candidates without another commit. Host roots persistently
fence legacy commits. Serving these APIs requires an explicit native binding.

`Vault::into_host_writer` consumes an existing handle, verifies an already
adopted v2 root and active native primary, then owns its OS Host lease until
the Vault drops. The private binding captures identity and primary epoch; it
cannot be deserialized from a page packet. Ordinary open/create remain
unbound. Binding never adopts, repairs, upgrades or falls back. Record-set
validation must remain enabled.

Bound `commit` acquires writer exclusion, rechecks actual publication/data/
authority and the captured primary, then uses witnessed publication.
`pin_current` and `find_receipt` also check under writer exclusion, including
service paths that replay without commit. A same-identity return at a later
epoch does not refresh the binding. Incomplete publication blocks new pins,
receipt replay and commits.

`host_writer_state` provides fresh native dispatch preflight. A past pin,
ManagedRoot access and domain request structs are not authentication grants.
The future Host must authenticate/scope callers, serialize operation/lifecycle
transitions, revoke sessions and gate reads/Provider side effects. Stop/drop
the bound service before acquiring the existing control/recovery coordinator
lease. This library does not implement those transport duties.

Legacy maintenance, purge/sweep, recovery adoption and lossy export remain
fenced through bound handles; Host-aware replacements need separate design.
No Workspace command, schema, dependency, reference-shell activation or
Runtime pin is added. See [synthetic evidence](../validation/MV-8-native-domain-writer.md).
