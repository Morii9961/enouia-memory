# ADR-MEM-113 — Owner Runtime capture pipe

2026-11-01. MCP-3 slice A, following ADR-MEM-102/111/112.

Freeze memory-runtime-capture-pipe-2 exclusively for automation-segmented-owner-v1
and memory-runtime-owner-capture-native-1. Reuse protected pipes, retained OS
peers, exact transitive schema handshake, framing limits and single-use binding.
Pipe-1 and Agent transports retain their contracts and refuse this profile.

A write captures then enqueues in separate transactions. Return capture and queue
receipts separately; a failed enqueue may accompany a genuine capture receipt.
Preflight never enqueues. Only a legal commit produces success. Caller-controlled
same-key replay after recovery repairs a capture/enqueue gap; no automatic resend,
reconnect or replacement key. Unknown closes enrollment and retains the original.

Status is a read of the exact original request, not a new dispatch. Native permit,
current policy and source scope authorize it before reading the worker journal.
Return only this producer's job keys, states and receipt/refusal counts. No source
text, memory IDs, foreign job counts or System/Owner reader is forwarded.

The bounded journal snapshot is read atomically without acquiring a worker lease.
Missing journal entries mean queued; dispatched remains uncertain, not success.
Background execution is slice B. No Provider, Agent capture, Runtime modification,
private data or required human review is activated.

See [contract](../contracts/OWNER_RUNTIME_CAPTURE_PIPE_V2.md).
