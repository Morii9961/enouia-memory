# UI v2 record contract, slice 1

Date: 2026-10-26. [ADR-MEM-93](../adr/093-ui-v2-record-layout.md).
Baseline: main `c9bf1be7819dba19915adf6031b1900c032b73d1`, verified by fetch.
Schema: [ui-records-v2](../../contracts/next/ui-records-v2.schema.json).
Native types: `enouia_memory_contract::next`.

This freezes the first slice of the Runtime UI v2 handoff, documents 01 and 02
dated 2026-10-26. Those documents are requirements; this ADR/contract defines
Memory's semantics. Morii's acceptance of these fields is still pending.

## Layout and storage

| Axis | Value / rule |
| --- | --- |
| New layout | `automation-host-v3` / `AutomationLayout::HostV3` |
| Document schema | 2; existing global schema-1 records unchanged |
| Admission rules | `admission-rules-3`; all Core-5 transitions/lineage still apply |
| Memory fields | `AdmissionInput.record_fields`, mandatory in both stored request and result on v3 |
| Source fields | `CapturedSource.record_fields`, mandatory on v3 |
| Entity/conversation catalog | `AutomationData.record_catalog`, mandatory on v3, empty at genesis |
| Owner ledger | `AutomationData.owner_actions`: required empty ledger/watermark on v3; machine deltas preserve it; reserved owner_action transaction refuses until G08 |
| Atomic capture | `AutomationVault::capture_with_records`: one source plus complete append-only catalog, same witnessed commit |
| Old roots | HostV1/2 forbid new fields/catalog; absent fields serialize identically to old fixtures |
| Old readers | Strict old layout enum rejects v3 descriptor before business interpretation; v1 rejects schema 2 |
| Existing native Core-7 read ports | Continue to accept HostV2 only; v3 reads are the next slice |

The catalog retains immutable revisions in insertion order. Each identity starts
at revision 1 and increments by one. A capture may append revisions but cannot
rewrite previous catalog bytes. Source retries require identical source and,
when supplied, identical catalog; changed work conflicts. No root header is
rewritten, existing root migrated, second primary selected or Runtime pin changed.

The new catalog's bound is 128 entity revisions and 128 conversation revisions;
Core-3's 128 sources/memories/receipts, 64 commits and byte caps still apply here.
Raising those limits is G01, not a claim of large-history support in this slice.
No catalog read endpoint is exposed yet. Later readers must reauthorize the
original evidence behind names, aliases, participants and conversation titles.

## Field mapping

| Stored field / type | Required meaning and bounds | Runtime handoff mapping |
| --- | --- | --- |
| `record_fields.title` | Nonblank, <=60 Unicode scalars, no controls; same source/language/revision as content; entailment remains a formation quality obligation | 01 G02; 02 §4.1 |
| `content_lang` on memory and source | BCP 47 conservative syntax subset, <=63 ASCII bytes; `und` means unknown, no host locale fallback | 01 G17; 02 §3/§4.1 |
| `event_time`, `valid_time` | Required `RecordTime` objects; endpoints exactly match Core-5 dimensions; distinct occurrence/applicability semantics | 01 G10; 02 §4.2 |
| `entities[]` | <=32 `{entity_id, role, evidence}` mentions; roles subject/participant/mentioned/place/project/organization; each evidence is from that memory's support list | 01 G09; 02 §4.3 |
| `EntityRecord` | Stable random `ent_` UUID, revision, person/project/place/organization/thing/pet/work, explicit real/fictional/unknown, name <=128, <=32 sourced aliases, <=8 evidence, nullable first_seen/last_seen, sensitivity and access policy | 01 G09; 02 §4.3 |
| Entity `project_id` | Required only for project entities; project mentions must match AdmissionInput.project_id; a non-null memory project needs a project mention | 02 §4.3 |
| `QuotedEvidence` | Existing EvidenceRef plus exact original `quote` <=500 scalars, source-copied `speaker`, scalar `[start,end)` within captured text | 02 §4.4; 01 G14 |
| `formation.prompt_version` | Required nonblank <=128; exact request/result binding | 01 G19; 02 §4.5 |
| `formation.import_batch_id` | Nullable typed ImportId; required for backfill, optional for live | 02 §4.5 |
| `formation.mode` | `backfill` or `live`; native seed fixes origin, model cannot alter it | 02 §4.5 |
| Existing formation provenance | System actor, rule, policy, producer_version, nullable model_version remain in exact request/decision; no duplicated approval field | 02 §4.5 |
| `annotations` | Nullable closed object bound to result ID/revision, model_version and prompt_version; never facts or owner feelings | 02 §6; 01 G19 |
| `formation_note`, `current_state_note` | Nullable <=500 scalars each | 02 §6 |
| `suggested_tags`, `tone`, `importance_guess_milli` | <=16 tags <=64; nullable valence [-1000,1000]/arousal [0,1000]; nullable importance [0,1000] fixed-point values | 02 §6 |
| `suggested_relations` | <=32 exact memory revisions; only related_to/same_event/continuation_of; no suggested causal truth | 02 §6 |
| `SourceRecordFields` | Language, nullable assistant_model <=128 (assistant speaker only), nullable internal conversation_id; upstream conversation ID requires conversation link | 02 §3 |
| `ConversationRecord` | Internal SessionId/revision; platform, opaque account alias, nullable upstream ID/title; <=32 participants/branches; independent unknown-capable start/end; import batch and explicit captured coverage | 01 G14; 02 §3 |
| Conversation `turns` | <=128 exact source revisions, local branch and turn IDs, positive per-branch unique sequence, nullable captured parent ref; all source links must resolve | 01 G14; 02 §3 |
| `turn_count` | Distinct captured turn IDs, not number of source messages or guessed platform total | 01 G14 |

Catalog labels inherit at least all referenced sources' sensitivity and the same
access policy. Source metadata preserves original speaker/class, timestamps,
timezone, upstream message ID, locator, parent/branch metadata, warnings and
attachment references in existing SourceRecord. These are data, not permissions.
One turn may include multiple source messages. Different branches never become
a single temporal chain; known source branch IDs must match the turn address.

## Time and evidence rules

`RecordTime` always has `from`, `until`, `precision`, `bound`, `tz`, `basis`.
Known `from` is UTC; `until` is nullable, exclusive and later than `from`.
Precision is instant/minute/hour/day/month/year/unknown. Non-instant known
precision requires a bounded interval. `within` means occurrence somewhere
inside; `span` means the full interval and needs both endpoints.

Unknown requires both endpoints null and basis unknown. It never falls back to
message/capture/import/commit time. Known basis is stated/inferred/source_time.
An explicitly selected source_time start must match a supporting source's
occurred_at. No implicit local timezone is chosen; missing `tz` is null. A later
initializer must record any owner-selected default in explicit provenance.
Timezone spelling is checked as a conservative IANA-style identifier, not as
registry membership or a guarantee about historical daylight-saving offsets.
Language accepts language[-Script][-REGION][-variant...] (unique variants);
extlang, extensions, private-use and grandfathered tags are outside this
conservative subset. Syntax does not certify detection or registry membership.

Evidence uses the original SourceRecord locator/object anchor, exact revision
and class. Additional scalar offsets locate the quote inside that captured
message. Exact matching includes punctuation, width, spaces and newlines.
The stored quote is the original text, not a normalized substitute. Hash-valid
assistant text remains model_claim. Source time is not learned time: the
containing immutable commit supplies actual learned_at, including backfill.

Reversion preserves historical title/language/time/entity/evidence claim fields
as well as Core-5's original claim and dependencies. It creates a new revision,
never rewinds CURRENT or deletion/policy watermarks. Annotations/origin describe
the new formation and stay distinct from the restored claim.

## Owner action shapes, without an execution port

`OwnerActionPlan` binds operation_id, exact SnapshotBinding, native owner,
planned_at/expires_at and one closed action. `OwnerActionConfirmation` binds
operation, canonical plan hash, owner, trusted local surface and confirmation
time. `OwnerActionRecord` freezes plan + confirmation + containing commit and
sequence; decoded JSON is not a witnessed receipt. The canonical hash uses the
repository's sorted pretty JSON plus newline, not an external compact-JSON hash.

| Action | Payload and effect reserved for G08 | Mapping |
| --- | --- | --- |
| correct | Exact target revision/hash, future source_id, actual assertion_text; only after native confirmation does G08 create manual_assertion and a System correction revision | 02 §5 |
| withdraw | Exact target, nullable reason; suppress reads/recall and invalidate derived outputs | 02 §5 |
| forget | Exact target + impact_preview_hash; confirmation uses existing deletion barriers | 02 §5 |
| attention | Exact target, protect/pin, nullable expiry; changes attention only | 02 §5 |
| theme_rename / theme_pin / theme_dissolve | Exact theme revision/hash and exact member refs, plus name for rename; overlap inheritance uses Jaccard >=0.5 only after current authorization | 02 §5 |
| entity_merge / entity_split | Distinct stable IDs, nullable reason; split has exact sourced evidence | 02 §5 |
| tag | Owner tag name and exact target refs, separate from suggestions | 02 §5 |

Plans and confirmations are structurally checkable but cannot grant authority.
`validate_owner_action_binding` takes native identity/current snapshot/time,
checks exact owner/digest/operation/window, and produces no success receipt.
System/Agent/Provider/Client cannot pass as owner even when all JSON fields
claim consent. Machine AdmissionInput and AutomationData have no owner-action
field; CapturedSource still refuses manual_assertion. G08 owns authenticated
planning, one-use confirmation, commit, operation lookup and unknown outcomes;
none is automatically retried or implemented by these pure types.
The root already stores the empty ledger and withdrawal watermark (initially 1,
then 1 plus the count of withdraw records), avoiding a later field addition.
The reserved owner_action change always refuses here; System capture/admission
cannot append or modify the ledger. G08 must adopt execution capability
separately; older builds refuse that transaction rather than silently skip it.

## Shared projection envelope and deletion propagation

`ProjectionMetadata` binds schema 2, document kind, algorithm, positive version,
exact snapshot (Vault/commit/primary/policy/deletion), withdrawal_watermark,
scope_hash, explicit coverage, <=128 original source dependencies and <=128
exact memory revision/hash dependencies. Coverage reports authorized inputs,
included inputs, completeness and bounded limit explanations. These counts
describe the authorized scope and cannot reveal excluded private objects.

The native reader must first authorize latest source/memory revisions, full
support/context/lineage dependencies, current policies and sensitivity. Only
then compute counts/facets, time buckets, names/summaries, graph endpoints,
activity or changes. Filter endpoints before edges/clusters; never expose blank
slots, hidden counts, IDs, or labels. No cached eligibility can bypass a gate.

`matches_current` rejects changed snapshot, withdrawal, scope, algorithm or
version; it does not replace live dependency authorization. All later cursors
also bind this metadata and their exact query/filter. Deletion or withdrawal
invalidates every dependent projection, including summaries that used context
rather than support; reconstruct only from live authorized dependencies.
Seven actual projections and their endpoints are subsequent slices, not
delivered by a metadata envelope. The empty withdrawal ledger is frozen now; G08 activates its native effects.

## Differences and handoff

- Exact quote matching is stricter than the suggested normalization fallback.
- BCP 47/IANA syntax is bounded and explicit; registry resolution/timezone
  conversion and owner-selected default timezone remain initializer/projection work.
- Tone/importance use fixed-point thousandths; they are optional suggestions.
- The catalog is an immutable bounded ledger, not an already scalable Sessions
  endpoint. Existing Core-3 runtime_event capture remains; export ingestion,
  agent-submission ingress and one-statement extraction are separately owned.
- Owner action records and projection rules are frozen shapes. G08 and the
  later projection slices must implement the native effects; no simulated
  owner click, approval or completed projection is claimed.

S2/S3 may use these field definitions after Morii confirms them. They must adopt
this exact revision and layout, retain source fidelity and preserve authority
boundaries. Real input/model permissions, initialization, new-root cutover,
Runtime native adapter/pin and UI acceptance require their separate slices.
