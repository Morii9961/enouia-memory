# Core-7 bounded native read model and simulation ports

Date: 2026-10-24. [ADR-MEM-91](../adr/091-authorized-memory-read-model.md).
Base: Core-6 `be6115348a737bcf1e429bcb0ee0d7ec2044e5ed`.
Schema: [read-model-v2](../../contracts/next/read-model-v2.schema.json).

## Capability and effect boundary

`memory-native-read-1` is a native, transport-neutral capability for explicit
HostV2 synthetic roots. Schema 2 alone is insufficient. Consumers must match
schema, document kind, capability and effect before interpreting fields. It
does not add a RecordKind, v1 command, MCP tool, client enrollment or authority
from parsed JSON. The existing native System/root must already be authenticated;
future Runtime adapters own their connection/window scopes and lifecycle.

Actual ports are `memory_page_native`, `memory_detail_native`,
`simulate_native_recall` and the separate pure `simulate_synthetic_recall`.
Native reads recheck live policy/deletion/source/lineage/sensitivity before
counts, body, source text, history or edges. An inaccessible selected identity
returns the same absence as an unknown one. Historical reads never revive old
permissions or deleted/stale source support.

| Effect | Meaning | Persistence |
| --- | --- | --- |
| native_read | Current authenticated root, read-only computation | No record, Context preview, experiment, receipt creation or index write |
| synthetic_fixture | Pure computation over explicitly supplied fixture values | No root/credentials/IO; no actual successful-commit claim |
| Saved experiment | Unavailable capability | No API/button contract exists |

The simulation result carries its effect, exact selection trace and qualified
rendered context. Native list/detail clone existing witnessed receipts; they do
not create them. Deserialization does not authenticate those tags or receipts.
Existing v1 `context_preview` still saves its old artifacts and must not be
renamed into either simulation port.

## List, scope and cursors

Pages contain latest currently authorized machine revisions, including staged
results when not explicitly filtered. They are not a count of verified facts,
owner approvals or the whole filesystem. `matched_in_scope` means the current
authorized latest set after form/adoption/event filtering, at most 128 items.
`unknown_event_time_in_scope` separately counts unknown event dates within the
authorized form/adoption set, before the optional event window. Unknown dates
are not silently placed on source/commit dates.

The optional event window uses event ranges; callers explicitly choose whether
unknown dates remain in that result. Stable identity order and at most 32 rows
per page support grids/lists. A cursor binds exact root snapshot plus filter
and profile hash. Changed root, deletion/policy watermark, filter or profile
refuses the cursor instead of silently rebasing it. Empty/oversized pages that
cannot progress return an error. Output JSON is bounded by a caller ceiling
between 1 KiB and 256 KiB; metadata headroom may shorten a page.

`complete_in_scope` and `next` describe that authorized bounded page, not global
life/history coverage. Source bodies, invisible versions, hidden endpoints and
their counts do not enter the page. Attention is computed from the returned
profile hash; `maintenance_worker_alive` only reports the native thread's running
flag, not successful ticks, product health or an end-to-end adoption claim.

## Meaning of each item

| Axis | Returned meaning / required display mapping |
| --- | --- |
| Formation | Exact System principal and rule/policy provenance; automatic machine formation, never Owner confirmation |
| Knowledge/adoption | attributed_statement/adopted means “subject said X”; hypothesis/uncertain/disputed remain System staged, not mandatory owner tasks |
| Validity | unspecified, in_range, not_yet_valid, expired computed at native authorization time; adoption alone does not mean currently valid |
| Attention | active/fading/archived/protected accessibility under the exact profile; not confidence, approval, completion or deletion |
| Occurrence/event | intended/unknown/reported occurrence and optional event range; plan is not an episode, cancellation not completion |
| Valid time | Applicability range, independent of occurrence, capture and learned time |
| Learned time | Actual containing immutable commit timestamp for this revision, resolved through the witnessed validated chain |
| Source captured time | Latest original dependency capture time; not event date, first global knowledge or this revision's commit time |
| Evidence/context | Original supporting citations separately from analysis/counter-source dependencies; neither summary repetition nor context count is independent corroboration |
| Receipt | Original stored result/commit/sequence/operation binding, after witnessed publication; list/detail never synthesize approval or receipt |

Conditions and exact derivation/reversion refs are retained. A row exposes no
`approvedByOwner` shortcut. Model understanding is not Morii's internal feeling.
These typed fields validate claims/labels, not arbitrary-language entailment.

## Selected detail and local neighbors

Detail selects one visible current identity and returns only its currently
authorized stored revisions, with their own source/commit times and receipts.
It is not an as-of knowledge/permission reconstruction. Old versions that no
longer pass current source/lineage gates are not exposed or counted.

Source details preserve exact SourceRecord, speaker/evidence class, native
subject attribution and optional original text. Text is absent by default and
requires an explicit selected detail request. This is local read permission,
not an external egress grant. Hash validation never treats UserStatement as
fabricated UserConfirmation.

Neighbors are exact derived_from/reversion links only. Both endpoints must be
in the current authorized peer/selected-history set; no hidden label/ID/edge
count appears. At most 32 links are returned. They describe derivation/history,
not inferred human relationships, causal truth, clusters or a global star map.

On byte pressure, detail drops optional source text first, then oldest returned
history entries, and marks coverage partial. If required current data still
does not fit, it errors. `complete_in_scope` concerns only current authorized
scope and budgets; it never promises all private historical variants exist or
were included. Required provenance is not replaced with invented defaults.

## Runtime Memory-region mount contract

| Mount owned by Runtime | This Memory capability supplies | Runtime responsibility |
| --- | --- | --- |
| Memory grid/list | Bounded page, exact ID/revision/snapshot, scope counts, state axes and cursor | Existing shell/navigation/token system; capability gate and explicit synthetic banner |
| Memory detail/source drawer | Selected detail, authorized history/source body, scope coverage | Explicit selection, late-response snapshot check, no fake owner-confirmed label |
| Time window | Event range filter and separate unknown dates, plus capture/valid/learned axes | Choose a labelled axis/timezone; no full timeline or imported-date fallback |
| Relationship neighborhood | Exact authorized local derivation/reversion edges | Label provenance rather than factual human relationship; no global graph claim |
| Recall explanation/simulator | Explicit effect, profile/algorithm/snapshot, current authorization time and trace | Distinguish fixture vs native read vs unavailable saved experiment; cancellation/lifecycle |

Capabilities explicitly unavailable: global timeline/graph, saved experiment,
legacy-owner history projection and vectors. Existing legacy APIs retain their
own owner history/current-only read semantics. No shared App/main/router/theme,
Runtime frontend file, dependency/pin, notification or default worker changes.

Real Runtime adoption is Core-8: match pin/lock/manifest, native adapter and
capability/version choice, then test Memory/Context/Sessions/Provider and
unrelated modules independently. Actual model quality, private migration,
independent recovery, real Provider activation and deployment are separate.
See [checks and limits](../validation/CORE7-read-model.md).
