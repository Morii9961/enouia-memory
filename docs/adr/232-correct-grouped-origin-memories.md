# ADR-MEM-232 — Correcting a memory formed from grouped origins

Date: 2026-11-28. Status: Accepted (bug fix found during Morii's clean-up of
study and work conversations).

## Context

The fresh initializer groups identical quotes from several messages into one
support origin (ADR-MEM-149). Such a memory's admission input carries
`support_origin_groups`, which must name sources in its `evidence`.

An Owner correction (ADR-MEM-103/155) builds the new revision from a copy of
the old input and replaces `input_refs`, `context_source_refs` and `evidence`
with Morii's own assertion source. It kept `support_origin_groups`, which still
named the replaced sources, so the contract refused the new revision
(`next.support_origin_groups`). Every correction of such a memory failed, in
Runtime as well as on the command line. Morii hit it while moving memories off
study and work conversations he wants deleted.

## Decision

A correction clears `support_origin_groups` together with the other
references to the replaced evidence. The corrected revision depends only on
Morii's assertion source (and the evidence of entities it still names), so
the original conversations can afterwards be deleted without hiding it.

## Verification

`identical_text_occurrences_retain_both_origins_in_one_support_group`
(`enouia-memory-init`, fresh driver) now also corrects the grouped memory,
deletes both grouped sources with the owner `Delete` delta and reads the
corrected memory back. Without the fix the correction plan fails with the
same violation seen on the real library.

## Consequences

No format or contract change. Runtime adopts by moving the pin; its
correction flow then works on grouped memories too.
