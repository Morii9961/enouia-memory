# UI v2 native process and detail reads

[ADR-MEM-101](../adr/101-native-process-detail-reads.md). Schema 2, read-2,
`automation-segmented-host-v1`; no stored record field or layout change.

| Native port on SegmentedMemoryReader | Coverage / result |
| --- | --- |
| `memory_activity_feed_native` | Actual witnessed capture/admission events, reverse publication time/event-ID pages; receipt and actor proof, no body |
| `memory_changes_since_native` | Initialize, unchanged-in-scope, changed IDs or explicit reload; exact current snapshot anchor |
| `memory_detail_native` | Current qualified row, recorded fields and up to 32 eligible historical revision summaries per page |
| `memory_conflicts_native` | Declared counter-source overlap with current eligible counterpart rows; not independent verification |
| `memory_revision_diff_native` | Adjacent eligible revisions, structural JSON field changes; empty when unavailable |
| `memory_neighbors_native` | Selected current/historical typed links, qualified theme suggestions only when requested |
| `memory_source_excerpt_native` | Selected current source metadata/fields and exact Unicode-scalar excerpt/hash |
| `memory_conversations_native` | Current eligible conversation metadata, captured turn/source counts |
| `memory_conversation_detail_native` | Captured turn fragments with source metadata/fields and bounded continuation |

See the Rust signatures for exact parameter types; these are native library ports,
not new IPC/MCP command names. Source and conversation ports require the default
memory filter, avoiding silently interpreting memory filters as source filters.
Activity can include source captures only with the default memory filter and no
selected memory ID. Memory event filtering selects current identities; eligible
historical events for those identities retain their actual historical axes.

## Authorization and versions

All calls use the authenticated System reader, native lock/lease gate and fresh
clock. `ProcessHeader` includes catalog selection, per-port algorithm/version 1,
parameter and permission hashes, authorization time and
`authorized_current_dependencies` scope. Catalog selection is now
`qualified-family-catalog-3`, version 3. Old catalog/projection DTOs retain their
old parsing semantics. Old cursor bindings are refused by the new reader.

The permission hash also binds active exact grants, ceiling and withdrawal
watermark, catching expiry of source-only grants. Ordinary pages retain their
snapshot/query/profile/authorized-set binding. Every result rejects stale
continuation after policy, source, metadata or deletion changes. Fresh
authorization timestamps can advance between otherwise compatible pages.

History is not an authorization bypass: the identity must currently be eligible;
each historical revision additionally needs its original exact active policy,
current original sources/entity aliases and current exact derived parents.
Historical revisions whose dependencies were superseded may be unavailable.
Hidden/deleted/ungranted IDs return the same empty selection shape; no historical
title, edge, blank placeholder or conflict count survives their exclusion.

`effective_sensitivity` is a verified record/dependency envelope, not a ceiling.
Native v3 catalog rows and graph nodes always carry it. Formation reason copies
recorded interpretation, rule/prompt/model versions, mode, import batch and note;
`note_is_machine_annotation` is true. Null notes are not synthesized later.

## Activity and changes

`NativePublicationProof` identifies the actual witnessed commit ID/hash, actor
and written-at time. Memory events include their real `MemoryReceipt`; material
captures cite their real capture commit. No raw source/memory body is returned.
Knowledge/adoption/change are independent, so adopted/staged/disputed labels need
not be inferred from event titles. The Owner event discriminator is prospective:
this slice has no publishing owner-action path and emits no Owner event.

Time-derived fading/archive events have no witnessed job in this layout and are
explicitly unavailable. Current deletion removes affected prior titles/events;
this feed is not a permanent owner audit. Clock changes never create fake jobs.

Changes query an indexed ancestor snapshot, not a guessed global sequence.
Append-only visible source/memory additions return at most 100 combined IDs.
Policy/deletion/metadata/revision changes, unknown ancestry, changed query/grants
or overflow return `reload_required`, empty IDs and a new anchor. A head change
can produce `unchanged_in_scope` plus `all_projections_invalidated=true`; this
does not disclose hidden IDs or hidden cardinality. The caller replaces its
anchor and reloads the affected read models instead of polling complete pages.

## Conflicts, differences and neighbors

Conflict basis is `recorded_counter_source_overlap_not_independent_verification`.
Counter-source refs must currently authorize; selected/counterpart rows preserve
their own knowledge/adoption, evidence and sensitivity. No semantic contradiction
detector, automatic truth choice or inferred owner confirmation is introduced.
Diff fields include title/content/axes/entities/sensitivity/origin/annotations/
evidence/dependencies where changed; values are structural data, not new facts.

Neighbors distinguish outgoing/incoming/undirected provenance relations and
`algorithm_suggestion` theme cooccurrence. Revision/correction/reversion links
use actual eligible targets and receipts, including selected-local prior
revisions. Historical local links require the default memory filter. Optional
suggestions use existing theme resource limits. At most 128 witnesses per
relation are returned with exact witness count and explicit completeness.
This is a detail surface; graph chunks continue their current-node semantics.

## Sessions and evidence jumps

Each immutable conversation revision is a bounded captured fragment. Read ports
union current-source fragments across witnessed revisions, retaining each
fragment's classification. Conversation and turn-row effective sensitivity
includes the metadata fragment's floor as well as visible source floors. A public source inside a private conversation
fragment does not expose that fragment's turn/branch metadata. Current caption,
participants and their dependencies must also be eligible. Parent links require
a current authorized source; no placeholder or hidden-ID edge is returned.

`visible_captured_turns` counts distinct turn IDs; `visible_captured_sources`
counts source rows, including branch variants. The embedded current conversation
record's `turn_count` describes that recorded fragment, not the union or platform
total. `platform_complete_not_claimed=true` is mandatory. Stable source-ID
pagination may differ from conversational order; display uses explicit branch/
sequence fields after bounded retrieval. No retained hidden-platform total exists.

Source excerpts use Unicode scalar positions matching quoted evidence. `start`
is 0–16,384, requested limit 1–4,096; output shrinks under the byte bound with
explicit `end`, total scalar length and completeness. Source metadata includes
content/anchor hashes and original locators/timestamps, but never duplicates raw
manual assertion or agent submission payloads. Extensions remain untrusted data.
Same empty response covers absent, deleted, stale and ungranted sources; Runtime
may say “source unavailable” without distinguishing those private states.

Pages obey the existing 1–100 row and 1 KiB–256 KiB caller bounds. A complete
selected row/header that cannot fit refuses instead of silently dropping data.
