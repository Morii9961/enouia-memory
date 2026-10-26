# Native Agent UI-record reads, version 1

2026-10-26. [ADR 097](../adr/097-agent-ui-record-reads.md).
Depends on frozen UI record commit `24a03e02bdf18094d60bc4d3268d380bd1b669a1`.
[Wire schema](../../contracts/next/agent-record-read-v2.schema.json).

## Capability and authority

`AutomationHost::agent_record_reader` issues a separate sealed native delegation
for **automation-host-v3** / document schema 2 / **memory-agent-record-read-1**.
The existing HostV2 factory still refuses this root, and Core-7's System reader
remains HostV2-only. No System port or Owner Workspace forwarding is used.
Native registration and OS enrollment, fixed subject/project/source/sensitivity,
independent source-read, current policy/primary/witness, source revisions/deletion,
expiry/revocation/lock generations and ancestor closure remain as in
[the Agent read foundation](MCP_AGENT_READ_V2.md).

A referenced entity's current name and every alias require all their exact source
revisions inside both current policy and the Agent source scope, with matching
access policy and sensitivity. Otherwise withhold the entire item before counts,
filtering, trace, history or neighbors. Historical source bodies or labels cannot
supply current authorization. This is not a cached System grant.

## Output and queries

Five tools retain their distinct schema-2 envelope/operations but require this
capability. `record_fields` on page/detail/recall is a revision-keyed array:
`{memory, producer_version, model_version, fields, entities}`. `fields` is the
exact stored MemoryRecordFields; `entities` contains authorized current
EntityRecord revisions. Base items retain separate knowledge/adoption, dimensions,
actual learned/capture times, System provenance, policy and witnessed receipts.
Nothing upgrades a subject statement or Agent claim into Owner confirmation.

Detail includes exact selected history metadata and `source_record_fields`
keyed by exact source revision. Without source-read, source metadata/body and
that array are empty. `memory_source` returns the selected SourceRecordFields
with bounded UTF-8 source text; this does not expose a whole conversation catalog.
Stored source conversation/message IDs remain available for later Runtime links.

Titles, language, entity mentions/names/aliases, quoted speaker/scalar ranges,
precision/bound/tz/basis, producer/model/prompt/import/live formation provenance
and optional annotations are preserved. Missing/unknown time remains unknown;
no date is synthesized from capture/import/learned time. Unsupported filters are
refused. Search adds entity_id, content_lang, precision, knowledge, validity and
attention; precision follows the selected time axis (learned/capture are actual
instant timestamps). Literal text includes authorized titles and entity aliases.
The current Core-7 native list limits remain 32 items and 256 KiB.

All field bytes count toward output budgets; pagination trims only at whole-item
boundaries and advances its snapshot/filter/grant-bound cursor. An item that
cannot fit is refused rather than represented as complete empty data. Recall
renders these exact fields into its qualified context before Core-6 selection,
so trace budgets include them. Native Context returns the same actual response,
including metadata, and refuses stale snapshots/authority. Reads do not reinforce
attention, allocate operations/receipts or persist Context.

Related still returns actual derivation/reversion only. Stored suggested-relations
annotations remain hints in their originating record; a global fact/suggestion
edge projection, topics and segmented-layout reads are unavailable here.

## Transport and compatibility

The Hello pins layout automation-host-v3, capability memory-agent-record-read-1,
schema 2, MCP 2025-11-25 and the normalized schema compatibility hash. Native
`Client::connect_records` and explicit stdio attach select it:

```text
enouia-memory-mcp --protocol automation-records-v1 --pipe <pipe-hash> --host-pid <live-host-pid> --vault-id <vault-id>
```

The operator must independently enroll that actual bridge process. Attach opens
no Vault or second writer. Old v1 and HostV2 clients refuse before business
queries, regardless of a shared schema major. Updated pins must match the full
hash; pin changes alone grant no new root permission. EOF/dead peer/lifecycle
loss closes the connection; unknown results retain the original packet/key and
never automatically retry or reconnect. Existing v1 and HostV2 routes retain
separate schemas and explicit support.

| Reader | host-current-v2 / schema 1 | automation-host-v2 / schema 2 | automation-host-v3 / schema 2 | segmented root |
| --- | --- | --- | --- | --- |
| Legacy six tools | Existing semantics | Refuse | Refuse | Refuse |
| HostV2 five tools | Refuse | memory-agent-read-1 | Refuse | Refuse |
| This record reader | Refuse | Refuse | memory-agent-record-read-1, read-only | Refuse |
| System native readers | Separate authority | Existing native gate | No Agent substitution | Only their explicitly adopted capabilities |

New temporary synthetic roots are sufficient for all tests. No private root must
be initialized for MCP development. The bounded HostV3 layout is not a claim of
20,000-record support; the parallel segmented interface needs its own committed
adoption. Runtime-only raw capture and Asia/Shanghai default provenance are
confirmed MCP-3 inputs, not a working hook in this slice. No actual Runtime edit,
provider/embedding/client configuration, migration, cutover or deployment occurs.
