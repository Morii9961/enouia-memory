# Native read-only historical planning · v1

MV-7 / [ADR-MEM-66](../adr/066-read-only-history-planning-host.md). This is an
optional embedded-host interface. The existing workspace IPC v1 has 47 commands;
neither the reference page nor Runtime starts using this method automatically.

## Host entry

```rust,ignore
use enouia_memory_workspace::{HostSurface, history::HistoryPlanRequest};

// Resolve the native caller window before deserializing/forwarding arguments.
// Quick Search must map to HostSurface::QuickSearch and will be refused.
let observation = workspace.plan_history(surface, &request)?;
```

`HistoryPlanRequest` is public, Serde camelCase, and rejects unknown fields.
All fields are required. IDs/revisions use Memory's existing typed contracts:

| Field | Meaning |
| --- | --- |
| `vaultId` | Current open Vault from `workspace_status.vault.vaultId` |
| `commitId` | The observed `vaultCommitId` envelope, or status `headCommitId`; refresh after any write |
| `importId`, `importRevision` | An observed `import_list.items` row's `importId` and `revision`; completed, nonduplicate import only |
| `unitIndices` | Strictly increasing, nonempty adapter unit indices; at most 128. Existing `conversationsCovered` gives the unit count for completed imports. Indices are zero based, not upstream conversation IDs |
| `branches` | `all` or `export_selected`; export selection is navigation metadata, never approval |
| `messagesPerBatch` | 1–256 messages; this is not a token or monetary budget |
| `maxPathReferences` | 1–16,384; counts repeated ancestors across paths |

The owner explicitly selects the import and units. Choosing `all` branches
authorizes local planning of that selection only. The method reads no caller
path and accepts no raw JSON, credential, actor, surface or destination field.
The host must not accept the separate native `HostSurface` from the page.

## Observation format

The result is a JSON object; `version` equals `history-host-plan-1`.

| Field | Meaning |
| --- | --- |
| `planId` | SHA-256 over version, exact request, raw hash and source-bound message metadata; deterministic for the same selection and commit |
| `vaultId`, `commitId`, `importId`, `importRevision`, `inputObjectHash`, `observedAt` | The pinned observation identity and local observation time |
| `selection` | Exact typed request |
| `totalUnits` | Recognized archive unit count, distinct from selected coverage |
| `archive` | Whole-archive `quarantinedMembers`, `preservedOnlyMembers` and deduplicated `warningCodes`; no member names or warning pointers |
| `units` | Selected unit rows: original `unitIndex`, parsed `messages` count, `threaded`, `unparseable` |
| `coverage` | Counts `dated`, `undated`, `unparseable`, `incompleteMessages`, `unavailableAttachments`; nullable observed `earliest` / `latest` |
| `messages` | Selected message table in adapter unit/message order; rows contain `unitIndex`, `messageIndex`, `sourceId`, `sourceRevision`, `contentHash`, `occurredAt`, `timePrecision`, `speakerRole`, `completeness` |
| `chronological`, `undated` | Zero-based indices into `messages`; chronological is navigation order, not branch order |
| `issues` | Original `unitIndex` plus `kind`: `ambiguous_message_id`, `broken_parent_chain`, `unknown_selection`, `non_monotonic_time` |
| `batches` | Branch-preserving summaries; fields described below |

A batch contains `batchId` (SHA-256 of version, planId and planner batch index),
`plannerBatchIndex`, `leaf`, `selectedInExport`, nullable `observedPathStart`,
`messages` and nullable `preceding`. `leaf`, message entries and `preceding`
index the observation's message table. Preceding context is on the same path.
`plannerBatchIndex` is meaningful only in this exact plan and selection; never
pass it to the older full-archive materialization helper as a global index.
IDs are observation references, not capabilities accepted by a writing API.

Coverage and messages always cover selected units, including unsafe graphs;
branch selection only filters the batch list. Unsafe graphs retain diagnostics
and source bindings but have no batches. Unknown times stay null; unknown
export selection is reported, never inferred. No source text, titles, archive
member names, upstream IDs or locators are returned. The metadata remains
private: hashes, times and role patterns are not public status telemetry.

## Bounds and invalidation

The Core refuses more than 4,096 selected messages or 16,384 current source
records, an archived input larger than 8 MiB, or a canonical UTF-8 result larger
than 256 KiB. ZIP planning allows at most 1,024 members, 8 MiB per member and
16 MiB expanded total, retaining existing ZIP safety checks. ZIP refusals use
the existing unsupported/partial adapter behavior; no silent partial result is
returned as a complete plan. Full-history scale needs a separate bounded-reader
slice, not a larger unmeasured default. There is no pagination in v1.

Reparsed archive member metadata and unit count must match the completed
manifest. A tighter ZIP limit cannot silently remove/reorder units while
retaining the same adapter name/version. Any such mismatch is refused.

Errors are the existing `WorkspaceError`: malformed selection is
`invalid_request`, oversized output/path budget is `budget_exceeded`, changed
Vault/head is `revision_conflict`, source-read denial is `permission_denied`.
Missing/revised/purged references follow existing storage error codes;
unmatched surviving source metadata is `broken_provenance`. Incomplete or
duplicate imports are refused. Errors carry stable rule names, never paths or
source content. Success never creates an operation or a durable job.

The method holds the shared lifecycle gate through the final freshness check.
Lock, close and switch wait for it; concurrent commits invalidate it before
return. The host still discards any old/late observation after a lifecycle or
commit change and must reobserve/replan before using a batch. It must not reuse
an old plan merely because the same Vault was reopened. Logical forgetting of
a canonical memory preserves its source; source/raw purge blocks the retained
archive both before and after physical file deletion.

## Runtime adoption

Adopt the fixed Core revision and matching surface aggregate, then add a native
adapter with window-derived scope and the above lifecycle/identity checks.
The method is synchronous; invoke it on a blocking worker rather than the UI
thread. There is no persisted operation or cancellation token in this slice.
Expose a read-only planning action beside the import report, with explicit unit
and branch selection, unknown/issue presentation, errors and reread behavior.
Do not import the internal planner, read Vault files or reconstruct locators.
Batch materialization, real extraction, semantic review and candidate publication
remain separate work. This interface performs no Provider calls, approvals,
personal-history import, MCP or cloud activation.
