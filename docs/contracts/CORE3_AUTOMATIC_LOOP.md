# Core-3 automatic loop in an explicit synthetic root

Date: 2026-10-24. [ADR-MEM-87](../adr/087-automatic-memory-transactions.md).
Base: Core-2 `defb2530092db8149e614a2290cc991fd423ece5`.

## Native entry and authority

`vault::automation::AutomationVault` creates only an explicitly selected empty
verified Windows NTFS root. Reuse owner-only ACL checks, managed bounded files,
the lifetime `indexes/host.lock` lease and short `vault/LOCK` transactions.
Descriptor `vault/vault.json` and `vault/CURRENT` use schema **2**, layout
`automation-host-v1`. This is a new synthetic authority, not a production
side database or a second writer for an existing Vault. It has no default path.

The descriptor fixes distinct native Owner and System principals and one Host.
The embedding native caller authenticates these identities independently;
`actor_type: system` in JSON is insufficient. Owner setup chooses the versioned
policy once. Capture requires trusted runtime event attribution; subsequent
memory formation/correction is System work with no owner ReviewRecord,
confirmation, candidate promotion or per-memory approval.

`host::automation::application::AutomationHost` owns this Vault and one
Core-2 synthetic coordinator at a separate job root. Native source capture,
policy change and source deletion serialize with application and invalidate
old jobs before changing the root. Lock closes dispatch/application; unlock
requires the exact native System and a valid root. Root replacement requires
dropping this Host and constructing a fresh instance with fresh permits.

The native orchestrator selects current source IDs/revisions and calls
`prepare_submit`; the Vault binds actual snapshot/policy/principal and allocates
operation/result identities. The seed contains selected original source text
and metadata, with ambiguous interpretation. This is not model instruction
trust. `start_worker` uses Core-2's single private worker and a weak completion
callback: durable generation -> fresh native validation -> atomic application.
No caller `apply_job` or human review is needed on the successful running path.
The returned thread handle remains native lifecycle responsibility. Core-2's
foreground scheduling and cancellation controls remain available.

This slice does not automatically subscribe to Runtime events, discover all
conflicts/targets, periodically sweep staged work, or start a worker by default.
Native orchestration submits new evidence/reassessment under the same path;
synthetic tests supply those events. Refusal or uncertainty creates no owner
inbox obligation. Production scheduling and semantic quality are separate work.

## Data, rules and limits

[storage-v2](../../contracts/next/storage-v2.schema.json) defines descriptor,
current, full-state immutable commit and publication intent. A commit embeds
the existing `AuthorityState` metadata with its own schema 1; this does not
upgrade the global record schema. Authority checkpoint, commit, policy/deletion
watermarks, data digest and parent pointer must agree. Every opened chain is
hash/shape/set/delta validated, not just its newest record.

| Stored value | Binding and meaning |
| --- | --- |
| CapturedSource | Complete runtime SourceRecord, original UTF-8 text/hash and native subject attribution; user statements stay UserStatement, never synthetic UserConfirmation |
| MachineMemory | Original prepared request, untrusted raw response JSON/hash, recomputed DecisionDraft, exact result revision and containing commit |
| MemoryReceipt | Original job/batch binding, operation, request/response/decision hashes, result, actual commit and sequence; returned only after durable witness |
| Policy/deletion history | Append-only full states and monotonic watermarks; deletion filters current use without rewriting old sources/approvals |

Pure set/delta validation reuses Core-1 evidence, dependencies, sensitivity and
target checks inside writer exclusion. All selected refs must be accounted for;
same-origin support cannot multiply, stale source/target/snapshot refuses,
and model output cannot replace native policy/actor/result/selection. A batch
contains 1–8 items against one snapshot; either all validate and commit or none
does. Two items cannot revise the same memory in one batch. Generation, polling
and external work never occur under Vault writer exclusion.

Rule 1 retains explicit target IDs. Rule 2 adds optional
`allow_revise_created`: native stored machine creation under the same policy ID
can authorize a future target without an owner granting every random memory ID.
It defaults false and is omitted when false; true with rule 1 refuses. Scope,
current policy revision/hash, expiry, subject/project, source/access/sensitivity
and exact target revision/hash still apply. Native provenance is an in-memory
observation, not a writable model field. Legacy owner targets always refuse.

Bounds: 128 source revisions, memories and receipts per state; 64 commits;
1 MiB per commit and 32 MiB cumulative chain. Source text is at most 16 KiB;
other next documents remain bounded at 256 KiB. Capacity refuses before
publication; no pruning, compaction or background database is introduced.

## Publication, restart and replay

1. Validate current descriptor/pointer/intent/witness and the whole chain under
   the sole Host lease plus short writer lock. Recheck current generation scope.
2. Flush immutable content-addressed commit with legal data and authority.
3. Atomically publish exact before/after intent, then CURRENT, then witness.
4. Return receipts only when publication finishes. Any uncertain write poisons
   that instance; it cannot issue further writes or claim success.

`open`, reads and receipt recovery never repair. Explicit native-primary
`recover` may only finish the validated intent whose observed CURRENT is its
before or after and whose witness is before (or already fully after). It
validates both states and legal transition, creates no operation/decision,
and never rewinds policy/deletion epochs. Conflicting or corrupt evidence
stays unavailable. Existing ADR-MEM-73 recovery/owner rules are untouched.

The Core-2 journal still owns original generation bindings. After reopen,
metadata-only bindings may locate an already committed batch even if the job
was invalidated on shutdown. Replay requires the same ordered hashes, live
current policy/source/deletion eligibility and original receipt. Changed raw
response bytes conflict even if JSON meaning is unchanged. Unknown generation
outcomes never cause blind Provider resend. A durable generated-but-unapplied
job can resume only with fresh exact current permits; stale inputs require a
new operation, never silent rebasing.

Process termination is tested; this is not proof against hardware power loss,
whole-filesystem rollback, malicious disk replacement or independent restore.

## Minimum current read and consumer matrix

The native bounded lexical reader first selects latest revisions and enforces
current policy/source/deletion/access/sensitivity eligibility, then query/limit.
Only adopted attributed statements enter context, prefixed with
`Subject stated (not independently verified):`. Staged uncertainty/inference/
conflict is withheld. A newer staged revision also withholds its older adopted
assertion while preserving both in history. There are no similarity scores,
verified-truth claims, full timeline, graph, vectors or public read endpoint.

| Consumer | V1 root | New synthetic root / rule 2 |
| --- | --- | --- |
| Existing embedded Runtime pin | Existing behavior | Reject descriptor/session capability before record decode/query; do not pass this root to the pin |
| Existing Host/MCP/Workspace | Existing semantics, including Owner-only writes | No new dispatch/tool/IPC fields; strict descriptor major gate refuses before business records/queries |
| Core-1/2 next-only reader | Original policy/draft/journal support | No business Vault support; old strict reader rejects populated flag/rule 2 before evaluation |
| This native AutomationVault/Host | Refuses V1 root without adoption or rewrite | Explicit synthetic constructor only; matched schema/layout/rule capability and native identity required |

Global SchemaVersion=1, RecordKind/AnyRecord, OWNER_ONLY and all 55 Workspace
commands remain unchanged. Raw unknown-major diagnostics confer no business
read/write capability. Runtime may later add a versioned native read adapter
that exposes machine vs owner formation, knowledge/adoption, current snapshot,
sources/context, policy/rules and witnessed receipt. UI filtering at the final
page is insufficient. Existing MCP grants cannot authorize automation writes.

## Migration, rollback and next boundary

No old records are migrated; exact legacy owner/review bytes remain untouched.
Disabling this opt-in path leaves its provenance intact and old clients on V1.
Never downgrade this root by changing headers or overwriting CURRENT. After V2
writes, use only validated forward recovery, or separately designed conversion
with monotonic deletion/policy barriers and independently verified restore.
The broader [migration design](CORE1_AUTOMATIC_ADMISSION.md) remains unexecuted.

Core-4 requires separately authorized bounded continuous Provider policy with
destination/scope/budget/expiry/revocation/in-flight rules, using fake transport
first. This slice supplies no network adapter, credential flow, real Provider,
embedding, private data migration, Runtime adoption, MCP tool or deployment.
