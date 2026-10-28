# UI v2 native projections: G03 → G05 → G04

[ADR-MEM-100](../adr/100-native-time-theme-graph.md). Read-2, schema 2,
`automation-segmented-host-v1`; accepted record fields are unchanged.

| Port on SegmentedMemoryReader | Result / meaning |
| --- | --- |
| `memory_time_buckets_native` | Explicit axis/timezone/day-or-month/civil window, complete bounded buckets, unknown count and separately scoped material layer |
| `memory_theme_members_native` | One stable recorded theme, qualified scores/bases, exact filtered member count and progressing continuation |
| `memory_graph_native` | Stable current-node window, compact nodes and typed edges in bounded chunks; candidate/retained counts per kind |

## Common selection

All use current native System authentication and lock state. The authorization
set is resolved before searching, labels, counters, empty spaces or relations.
Expired/revoked grants, deleted/stale source/parent revisions, hidden entity
label/alias evidence and native sensitivity ceilings exclude dependent input.
No index/receipt/record is written by a read. There is no authoritative saved
projection; every result is rebuildable from the witnessed record chain.

`ProjectionHeader` carries the catalog selection (exact snapshot, current
withdrawal watermark, authorized-set/query/profile hashes, evaluation time),
algorithm and version, extra-parameter hash, fresh authorization time, declared
scope, matched/included inputs and limits. Current catalog selections are
`literal-family-catalog-2`, version 2; parsing an old DTO is not authentication
or continuation under the new algorithm. Cursor binding freezes validity/
attention time but never freezes permission checks. A changed root/scope/query/
ceiling/profile/parameter refuses continuation. Fresh authorization timestamps
may advance between pages without invalidating an otherwise identical binding.

Caller row limits remain 1–100, canonical JSON byte limits 1 KiB–256 KiB.
A partial output always progresses, with explicit `next`; an item/minimum
header that cannot fit refuses. `complete_in_scope` on graph headers is false
if the node window or edge retention discards candidates. Page continuation
separately describes whether the selected retained result has been delivered.
Counts never describe inaccessible inputs or the owner's whole life.

## Time

`TimeBucketQuery` requires `axis` (event/valid/learned/source_captured), a bundled
IANA `timezone`, `granularity` (day/month), and canonical YYYY-MM-DD from/until
civil dates, with exclusive until. Month windows start/end on the first day.
No global/default timezone is guessed; unknown zone IDs refuse. Existing
`CatalogFilter` time windows must be absent on this port to avoid mixing axes.
All its other filters apply to the memory set. At most 366 buckets are returned.

Civil midnight uses Jiff compatible gap/fold resolution and `start_of_day`.
DST days may be 23/25 hours; a skipped civil date has from==until and no instants.
`timezone_database` identifies exact Jiff 0.2.37 / jiff-tzdb 0.1.8 and its
actual IANA release (2026c), independent of the host OS. The exact Cargo.lock and integration aggregate bind their content.

`possible_records` includes a known point in that bucket or a known interval
possibly overlapping it. Both `within` uncertainty and `span` are conservatively
counted by overlap: this never asserts one occurrence per day, a known start,
or a continuous event for an uncertain date. The rule is explicitly
`possible_overlap_half_open_not_additive`. Staged/adopted counters are separate
subsets on that same selected memory axis. Unknown time is counted after other
filters and outside known buckets, never replaced by capture/learning time.
Zero means no matching currently authorized input in that selected interval.

`captured_material` always uses the source capture axis. Its declared scope is
`authorized_current_sources_unfiltered`, with `authorized_material_in_scope`;
these counts include authorized unformed material and do not follow memory
text/entity/theme filters. The output states this distinction explicitly.

## Themes

Family ID is the current recorded theme Memory ID; reading/recomputing never
creates another ID or memory revision. Exact derivation members score 1000 with
`recorded_derivation`. Other authorized records sharing a non-subject entity
score 750 with `shared_entity_suggestion`. No hidden row/alias/entity dependency
participates in a family name or count. A seed is not its own member. This is
a deterministic suggestion algorithm, not semantic clustering or truth.

Members need not be in the theme record's bounded `derived_from`. Membership
can exceed 128 and is paginated by stable current Memory ID. Catalog filters,
facets and statistics use exactly this algorithm, replacing the v1
`recorded_derivation_only` interpretation with `recorded_shared_entity_families`.
One family is computed on demand; full theme facets/aggregations refuse beyond
16,777,216 authorized-input-by-theme checks. Ordinary un-faceted catalog pages
remain available without expanding the theme cross product. No partial count
is represented as exact when a resource bound is exceeded.

Until G08, `owner_named` and `pinned` are false. G08 will apply actual confirmed
owner records and preserve names/pins/dissolution across disposable recomputation,
with recorded-member overlap checks. This phase does not accept caller/model
owner annotations or claim owner-name inheritance has been executed.

## Graph

`GraphQuery.node_limit` is 1–5,000; stable-ID window selection reports all
matching authorized inputs versus included nodes. Filter text and all state/
entity/project/theme/time axes are the catalog's current semantics. Nodes are
compact: exact TargetRef, title, form, knowledge/adoption/validity/attention,
rich event time, entity IDs and qualified theme IDs. No body or coordinates.

Edge kinds: derived_from/reversion when the exact selected endpoint is current;
same_conversation and same_entity from recorded metadata; theme_cooccurrence
as an `algorithm_suggestion`. Each carries an explicit witness. Relation facts
are provenance facts, not corroboration of memory contents. Historical revision
and owner correction relations are not invented as self edges between identical
current IDs; G15/G08 provide those later detail/owner surfaces. Semantic neighbors
remain explicitly unavailable; there is no embedding or model call.

Candidates are counted only after both endpoints are authorized and selected.
Different witnesses make distinct relations. A clique's candidate count is
calculated without allocating every edge. `edge_limit_per_kind` is 0–20,000;
retained edges use deterministic kind/witness/endpoint order. At most 262,144
grouped memberships and 16,777,216 selected-input-by-theme checks are allowed;
reduce the window/filters if these bounds refuse. Every page declares each
kind's candidate/retained counts. Filtering/deletion cannot leave hidden empty
node slots, endpoint edges or cluster names, and any changed snapshot refuses
old continuation. Frontend layout and visual acceptance belong to Runtime.


## ADR103 native read qualification

The later Owner slice emits selection `owner-qualified-catalog-4` version 4.
Process headers use algorithm/version 2. Restart older cursors. Current scoped
receipts omit global commit sequence; current conversation metadata omits raw
turn arrays/totals/branch lists and ranks authorized captured turns densely per
branch. Actual Owner marks, family/identity overlays and provenance require the
same fresh authorization and native action records. Stored records and legacy
v3 fixture parsing remain unchanged. See [Owner contract](UI_V2_OWNER_ACTIONS.md).
