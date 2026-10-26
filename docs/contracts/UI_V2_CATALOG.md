# UI v2 scalable catalog: G01, G06 and G23

Date: 2026-10-26. [ADR-MEM-94](../adr/094-segmented-native-memory-reads.md).
Depends on the accepted [record fields](UI_V2_RECORDS.md), commit
`24a03e02bdf18094d60bc4d3268d380bd1b669a1`. No record field is replaced.

## Explicit physical layout

`automation-segmented-host-v1` is a new empty-root layout, document schema 2.
Use `automation::segmented::SegmentedAutomationVault`; old AutomationVault
entry/recovery paths reject it. Old HostV1/2/3 roots keep their original limits
and semantics and are never upgraded by this code. Runtime and MCP must match
layout, exact code revision and read capability before dispatching anything.

The immutable `SegmentedCommit` contains a bounded delta and exact parent,
sequence, native actor/time, monotonic policy/deletion watermarks and existing
Host authority. Its authority checkpoint binds the parent hash plus delta;
the pointer hashes the complete canonical commit. The complete linked chain is
verified/replayed on open. Deltas are genesis, owner policy revision, System
source/catalog capture, System machine batch, and owner deletion. Owner action
publication remains reserved/unavailable until G08.

| Bound | Value |
| --- | --- |
| Source / machine revisions | 65,536 each; immutable histories count toward capacity |
| Current policy identities | 1,024, with exact independent subject/project/source grants |
| Commit chain | 65,536 commits, 1 GiB cumulative canonical bytes |
| Single commit | 4 MiB |
| Capture batch | 1–64 source revisions; at most 128 entity/conversation revisions each |
| Machine batch | 1–8 original requests, raw responses and generation bindings |
| One input / policy source selection | Original Core-1 bound of 128; no wildcard |
| Exact derivation closure | At most 128 visited current parent identities; stale/cyclic refs refuse |

There is no second database or unversioned authority sidecar. Native indexes
hold exact source/current-memory revisions and materialized lowercase title/
content search text. They update when a witnessed delta is applied and rebuild
from the chain on reopen. Search/facet/statistics never persist or reinforce
anything. CURRENT/intent/witness/descriptor are checked before each operation
under the lifetime Host lease; a mismatched head refuses rather than serving
an old cached result. Immutable segment corruption is checked on reopen.
Whole-filesystem rollback, physical purge and independent restore are not proven.

Model generation happens outside these methods. `prepare` fills native scope,
snapshot and operation/result IDs. `evaluate` is the existing pure label rule,
not a model. `apply_batch` rechecks exact generation, source/lineage, policy,
record fields and operation bindings before the short witnessed publication.
Original raw response bytes and learned commit time remain recoverable. Same
exact batch replays only original receipts under current authorization; changed
work conflicts. Publication uncertainty fences the instance. Explicit native
primary recovery may finish only the fully validated pending intent.

## Native read-2 entry and outputs

`automation::catalog::SegmentedMemoryReader::from_authenticated_system` requires
the separately authenticated native System for that root. This constructor
does not authenticate a JSON actor. No v1 IPC/MCP command or default Host route
is added. The new reader owns a lock/unlock gate and retains the root's lease.

| Method | Effect / output |
| --- | --- |
| `memory_page_native` | Latest currently authorized records, stable ID order, <=100 rows, exact matched/unknown counts and explicit continuation |
| `memory_facets_native` | One axis's exact values/counts with bounded continuation and declared count scope |
| `memory_stats_native` | Exact filtered total and nine axis groups with bounded value coverage |

All are `memory-native-read-2`, schema 2, effect `native_read`. They do not claim
selected history/source detail, Context delivery, saved experiments, global
timeline/graph or legacy owner history. Existing read-1 ports remain unchanged.
Runtime must explicitly adopt read-2; it cannot feed these roots to an old pin.

Every selection carries algorithm `literal-alias-catalog-1`, version 1,
profile hash, exact snapshot, withdrawal watermark, authorized-set hash,
query hash and evaluation time. The owner ledger is currently empty, so its
withdrawal watermark is exactly 1. G08 will implement native changes.

Rows retain Core-7's System principal, policy/rule, qualified knowledge and
adoption, formation dimensions, valid/learned/source-captured times, original
support/context refs and witnessed receipt. They add title/language, rich time
and entity identity/role. Machine adoption is never owner confirmation or
independent verification. Title/entailment and model quality remain separate
from these structural checks.

The scope is **authorized_current_machine_revisions**, not every file, all
source history, private records or lifetime platform coverage. Counts are
exact within that scope; no hidden count, lower-bound cutoff or silent 128-row
truncation exists. Storage capacity errors refuse publication. Pagination
completion describes that result set, not real-history completeness.

## Authorization, filtering and pagination

Before counts, text search, aliases, names, facets or theme membership, select
latest identities through current active/valid exact policies, native ceiling,
source deletion/revision/access/sensitivity, memory deletion and exact live
derivation closure. Entity display names and every alias evidence dependency
must also be allowed by that memory's own policy; another grant cannot lend it
authority. A hidden label dependency withholds the dependent row conservatively.
Changing policy/root/source/parent/deletion cannot revive a historical grant.

`CatalogFilter` has literal text, form, state, knowledge, adoption, validity,
attention, entity, project, theme and an optional UTC time window/axis. Chinese
substring and English lowercase literal matching use title/content plus
currently authorized entity names/aliases and explicit profile cue aliases.
Whitespace-separated terms are ANDed; each term's explicit aliases are ORed.
This is a materialized literal index, not vectors, stemming or model inference.

Time axes are event, valid, learned and source_captured. Unknown event time is
never replaced by capture or learning time. `include_unknown_time` is explicit.
Unknown count is computed after non-time filters, before the time window.
Known intervals use half-open overlap; instant points use membership.

Cursor selection binds snapshot, live authorized-set hash, algorithm/version,
filter/profile/sensitivity and the first page's evaluation time. Validity and
attention use that explicit frozen evaluation time for consistent paging;
policy expiry/ACL/source/deletion authorization always uses the fresh native
clock. A changed authorized set or snapshot invalidates continuation, as do
different filters/profile/ceiling. Cursor hashes are not authentication or an
egress grant. Unknown/inaccessible filter identities yield the same empty set.

Caller limits are 1–100 rows and 1 KiB–256 KiB canonical JSON. Long content or
metadata may shorten a page; every partial page progresses from its final ID.
If a required row/header cannot fit, the call refuses instead of returning an
empty nonprogressing cursor or silently dropping required provenance.

## Facets and statistics

Nine axes: form, formation state, knowledge, adoption, validity, attention,
entity, project and theme. Page facets use **other_filters**: all other selected
axes/text/time remain applied, while the counted axis is unselected. Statistics
groups use **matched**: all filters remain applied. A null formation state is
`not_applicable`; no project is `none`. Entities/theme memberships may overlap,
so their counts are not additive across values.

Embedded groups include at most 16 values per axis; `total_values` is exact,
and `next` explicitly signals incomplete value coverage. The facet port pages
through up to 100 values at a time under the same selection. Byte pressure
shortens groups with a valid continuation or refuses if minimum metadata does
not fit. Counts of returned values remain exact.

Theme membership currently means **recorded_derivation_only**: an authorized
current theme's exact `derived_from` references, with both endpoints visible.
Its title is an interpretation, not a newly inferred human relationship or
the later G05 family projection. G05 must add larger rebuildable memberships
before Runtime can claim whole-corpus topic families. Time buckets, graph,
activity/change feed, conflict groups and owner actions remain subsequent work.

## Synthetic acceptance

The explicit 20,000-record test is ignored in the default quick suite and was
actually run separately with offline/locked release tests. It forms real
source/policy/machine delta commits in a new temporary root, traverses all
pages without omissions/duplicates, compares filtered totals and statistics,
checks deletion/policy cursor invalidation, then closes/reopens and traverses
the remaining set. This is native synthetic evidence, not real initialization,
external Provider quality, Runtime UI acceptance or deployment.
