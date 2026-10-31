# MCP-3 captured formation queue

2026-10-31. Base 68ca1cc. [ADR-MEM-111](../adr/111-captured-source-formation-queue.md),
[contract](../contracts/CAPTURED_FORMATION_QUEUE_V1.md). Only newly allocated
synthetic roots and owned child processes. No private root, Provider, embedding
or model call.

## Actual evidence

Four new cases in `crates/enouia-memory-host/tests/owner_runtime_capture.rs`:

- Admission and reopen: a real owned Runtime peer captures and enqueues. The
  receipt says `queued` and validates against the closed schema; the stored
  segment validates as an `enqueue` delta. Repeat and capture replay return the
  same receipt with no new commit. Jobs survive reopen, zero memories exist, and
  the capture receipt keeps formation_scheduled=false.
- Capture as data: refused with no commit for the wrong actor, preflight value,
  captured=false, old capability, other Vault, wrong revision, another request's
  sources, forged request hash, forged producer and a policy outside the source
  pool. Changed sources or policy under an admitted key conflict. An old-layout
  root refuses.
- Order: turns enqueued out of order come back in native turn order, ahead of a
  later session. Revoking the first turn's policy holds the rest of that session
  while the other session continues. Deleted sources drop out and release it.
- Unknown publication: faults at StagingWritten and AfterCurrent report
  OutcomeUnknown. After recovery the same key yields one entry and a stable
  receipt, whether or not the first attempt persisted.

Checks, offline and locked:

| Check | Result |
| --- | --- |
| Serial workspace tests | 832 passed, 0 failed, 4 ignored |
| Focused `owner_runtime_capture` after final edit | 9 passed, 0 failed |
| Fmt and strict workspace/all-target clippy | Pass on the final tree |
| Python 3.12 / jsonschema 4.26.0 | 63 schemas, including 8 new queue cases |
| Runtime surface | Logged, Workspace IPC remains 58 commands |

Strict clippy first failed on one needless `.into()` in the new test helper. That
failure is kept here and not counted as a pass. The workspace run predates that
one-line test edit; only the focused file, clippy, fmt and surface were rerun
afterwards. Product code is unchanged since the workspace run.

A self-review before the final checks added the writer lock to queued_captures,
matching the other System reads. The first check run was stopped for that edit
and is not counted.

## Not proven

Refusal by an older binary follows from the closed delta enum and was not run
against an old build. Real concurrency between a worker and enqueue is untested
because no worker exists. Nothing here measures formation quality.

## Support matrix and remaining work

| Root | Enqueue | queued_captures |
| --- | --- | --- |
| automation-segmented-owner-v1 | Owner capture only | System only |
| segmented-host-v1 | Refused | Empty; no entry can exist |
| Other layouts | Not supported | Not supported |

Next work is the shared formation worker: dequeue, completion state, generation
and live priority over backfill, reusing this queue from the initializer. The
Owner Runtime wire, Agent hosts, `conversation_capture` and `memory_note` follow.
Runtime hook/config/pin/UI, real Provider/embedding, private data and daily
formation acceptance remain undelivered.
