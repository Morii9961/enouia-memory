# Owner Runtime capture pipe v2

2026-11-01. ADR-MEM-113. Protocol memory-runtime-capture-pipe-2, record schema 2,
layout automation-segmented-owner-v1, capability memory-runtime-owner-capture-native-1.

Call: closed {mode: preflight|write|status, request: original RuntimeCaptureRequest}.
Reply: closed {request_id, mode, capture, queue, jobs, error}; unused values null.
Capture and queue have their native closed receipt schemas. Capture retains
formation_scheduled=false. Queue state queued means durable admission only.
Write returns capture plus queue on success; enqueue failure returns capture plus
error. Capture failure has only error. Preflight returns only capture or error.
Status returns only jobs or error, never captures/enqueues. Jobs are bounded by
the original request (at most eight): {job_key,state,receipts,refused}. State is
queued, dispatched, forming, formed, no_memory, rejected or analysis_unknown.
Receipts is a count of genuine memory commits, not an adoption assertion.

Errors: outcome_unknown, permission_denied, invalid_request, capacity, unavailable.
Any transport uncertainty retains the exact call and closes the client. Unknown
publication also closes native enrollment. Recovery and same-key replay require
explicit fresh native setup; neither endpoint retries automatically.

Hello exactly binds protocol/schema/layout/capability/vaultId/compatibilityHash.
The hash covers the wire, Owner capture, original capture, common and automation
schemas plus framing. Retained OS peers are checked before and after IO. One
connection per enrollment, at most 256 calls. Hello/welcome <=4096 bytes, calls
<=256 KiB, replies <=1 MiB, existing bounded deadlines. No wire enrollment,
lock/unlock, root selection, policy expansion or recovery command exists.

| Consumer/root | Result |
| --- | --- |
| Owner root + explicit Owner native capability + pipe-2 | Capture, queue, own status |
| Owner capability + pipe-1 | Refused before binding/body |
| Old native capability + pipe-2 | Refused before binding/body |
| Agent/Workspace profile or mismatched hello | Refused |
| Expired/revoked/dead/wrong peer or stale policy | Refused; no new authority |
| Foreign request/hash/producer, deleted or out-of-scope source | No status disclosure |

No Runtime hook/config/pin, real analyzer, memory_note or mandatory review queue.
