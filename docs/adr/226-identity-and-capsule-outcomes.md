# ADR-MEM-226 — Owner outcomes for entity identity and capsules pass their own check

Date: 2026-11-27. Status: Accepted (fix). Corrects ADR-MEM-172 and ADR-MEM-173,
which added the `entity_identity` and `capsule` owner actions but not to the
outcome's native check. Same class as ADR-MEM-224.

## Context

Committing an owner action returns a `NativeOwnerOutcome`, and
`NativeMemoryOwner::outcome` returns the same document when a lost reply is
recovered. Its native check accepted only the actions that existed before
ADR-MEM-172. The outcome of confirming an identity (`entity_identity`) or
saving a capsule (`capsule`) therefore failed `validate()`. The schema
already allowed both. The vault does not check its own outcome, so Morii's
identity confirmation on the real library committed. A caller that parses
the outcome with `parse_document`, such as an adapter or the reference
shell, gets `Invalid` for a commit that succeeded.

## Decision

The outcome check accepts `entity_identity` and `capsule`. Nothing else
changes.

## Verification

The `execute` helpers of the capsule and entity-identity host tests now
check that every committed outcome passes `validate()`, and that recovering
it by operation returns the same document. Before the fix the capsule test
failed at its first save.
