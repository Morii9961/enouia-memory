# Captured formation queue v1

2026-10-31. [ADR-MEM-111](../adr/111-captured-source-formation-queue.md).

Native Vault API on automation-segmented-owner-v1. No IPC, pipe or MCP route.

```text
enqueue_captured(system, capture, policy) -> CapturedQueueReceipt
queued_captures(system) -> [QueuedCapture]
RuntimeCaptureHost::enqueue(capture) -> CapturedQueueReceipt
```

CapturedQueueReceipt: schema_version 2, kind `captured_queue_receipt`, queue_key,
job_keys, state `queued`. It means the jobs are durable, not that anything
formed. Stable across replay and reopen. Closed schema:
`next/runtime-owner-capture-v2.schema.json#/$defs/queueReceipt`.

Stored delta: `{"change":"enqueue","entry":CapturedQueueEntry}` in
`next/catalog-v2.schema.json`. The entry holds queue_key, session_id, request_id,
request_hash, producer, policy, ordered source revisions and one job_key per
source. It holds no text, role, timezone, generation request or response.

- `queue_key = digest("captured-queue-1", vault_id, producer, request_id)`.
- `job_key = digest("captured-job-1", queue_key, source_revision_ref)`.

| Case | Result |
| --- | --- |
| Not an Owner root, or not the System actor | Denied |
| Old capability, preflight, not captured, other Vault | Denied |
| Revision, provenance, session, order, policy, pool, sensitivity or deletion mismatch | Denied, no commit |
| Same key and same binding | Original receipt, no commit |
| Same key with changed request hash, sources or policy | Conflict, no commit |
| Publication failure | OutcomeUnknown; recover, then repeat the same key |

RuntimeCaptureHost::enqueue also requires a live retained peer, the enrolled
producer and this enrollment's scope hash, and uses the enrolled policy.

queued_captures returns refs only. Order, deletion and policy-hold rules are in
the ADR. A job follows its policy's current revision, so a routine policy update
does not strand it.

## Worker

[ADR-MEM-112](../adr/112-captured-formation-worker.md). Native Rust only.

```text
CapturedFormation::open(vault, system) -> CapturedFormation
CapturedFormation::step(analyzer) -> bool
CapturedFormation::status() -> [FormationJobStatus]
CaptureAnalyzer::analyze(job, source) -> CaptureAnalysis
Initializer::step_with_captures(captures, analyzer) -> bool
```

`step` returns false when nothing is dispatchable. CaptureAnalysis is
Statements, NoMemory, Paused or Unknown. One step performs one action: dispatch,
prepare one statement, apply it, or settle the job.

FormationJobStatus carries job_key, state, receipts and refused count. State is
`forming`, `formed`, `no_memory`, `rejected` or `analysis_unknown`. Only `formed`
with a receipt means a memory was written, and adoption still comes from the
evaluator. `dispatched` exists only while a call is in flight.

The journal `sync-state/captured-formation-1.json` is private, version 1, bound to
the vault id and descriptor hash, and refused if either differs. The lease is
`sync-state/captured-formation-1.lock`; a second worker on one root gets Busy.
