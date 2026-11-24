# ADR-MEM-202 — A short lookup that finds nothing says so

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The read session answers a lookup by ID with `data: null` and `error: null`
when nothing with that ID is visible. ChatGPT often has stale or mistyped
IDs, and it got a bare null it could not interpret, from any of these tools:

- `memory_read` or `memory_related` with an unknown memory ID;
- `memory_source` with an unknown source ID.

The null is deliberate: it does not reveal whether the item does not exist,
was deleted, or lies outside the grant.

## Decision

When a short call to `memory_read`, `memory_related` or `memory_source`
answers with null data and no error, the connector adds a `note` to the
result. The note says that nothing with this ID is visible to the connector,
and that the ID is unknown, was deleted, or lies outside Morii's read grant.
The text content is rewritten to match. The result is still not an error.

The wording keeps the read session's neutrality: it names all three causes
and does not say which applies.

## Verification

`remote_daily_process` (real binary) calls each of the three tools with an
ID that does not exist. Each result is not an error and carries the note.
The full Windows host suite passes (58 test binaries).
