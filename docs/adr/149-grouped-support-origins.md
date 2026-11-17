# ADR-MEM-149 — Retain repeated occurrences in one support origin group

Date: 2026-11-17. Status: Proposed; synthetic implementation only until merged.

Morii authorized a bounded contract addition that keeps every selected quote and
original occurrence while treating repeated content as one support origin group.
This continues the local delivery route; it starts no MV stage.

## Problem

ADR-MEM-85 rejects different SourceIds with identical content or the same upstream
message. That prevents copies from multiplying support, but also refuses an
authored statement quoting distinct original occurrences of the same text.
Dropping a quote, replacing its source identity, or silently calling it context
would change the authored input. Multiple ranges on one SourceId already work.

## Decision

Add optional `AdmissionInput.support_origin_groups`, omitted when empty. Each
group contains two or more exact SourceRevisionRefs; all groups together contain
at most 128 distinct references, within the existing source/evidence bounds.
Members and groups have canonical sorted order. Only supporting sources belong;
context-only references cannot be inserted to supply extra support.

The trusted evaluator derives connected components among distinct supporting
SourceIds: equal content hashes or equal non-null upstream identities
(provider/account/conversation/message) connect two sources. It accepts an
explicit grouping only when it exactly equals those components. Missing,
invented, split, merged, stale or overlapping groups fail. Empty/absent groups
retain the previous `next.duplicate_origin` refusal when collisions exist.

Every retained quote keeps its own SourceId/revision, anchor, range, speaker,
Raw identity and dependency. All existing availability, deletion, policy,
sensitivity, attribution, formation, duplicate-range and transaction gates
apply to every occurrence. The fresh authored initializer derives the explicit
groups from its selected sources; it never rewrites the original manifest.

Each explicit component counts as one support origin. Ungrouped distinct
supporting SourceIds each count as one. Occurrence/range counts are not counts of
independent corroboration. This addition changes neither interpretation nor
knowledge/adoption outcomes, and introduces no confidence score or truth claim.

## Compatibility and verification

Legacy inputs serialize identically with this field omitted. Older closed
readers reject grouped inputs rather than silently discarding the grouping.
Runtime must adopt the new exact pin and surface aggregate before opening such
decisions, preserve the field through adapters, and never label occurrence or
quote counts as independent support. No Runtime checkout or UI changes here.

Synthetic checks must reproduce the old refusal, test canonical grouping and
transitive collisions, reject malformed/incorrect groups and unrelated merges,
retain the old ungrouped-copy refusal, and form/reopen/replay/query both original
occurrences with unchanged quotes. Schema cross-checks and the regenerated
Runtime surface accompany the contract change. Real input waits for merge;
whole-journal capacity, actual formation and recovery remain separate gates.
