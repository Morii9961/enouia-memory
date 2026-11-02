# Runtime capture local pipe v1

2026-10-27, ADR-MEM-102. Extends the native source-only building block in
[RUNTIME_CAPTURE_NATIVE_V1](RUNTIME_CAPTURE_NATIVE_V1.md). No public MCP tool,
Runtime hook, live formation worker or private root is activated here.

## Native binding and versions

Only document schema 2 / automation-segmented-host-v1 accepts
memory-runtime-capture-native-1. The local transport is explicitly
memory-runtime-capture-pipe-1, independent of memory-agent-pipe-1/2/3, MCP
2025-11-25 and Workspace IPC v1. A source wrapper retains SourceRecord schema 1
plus the frozen SourceRecordFields; that does not make old roots capture-capable.
Unsupported root/layout/capability refuses before business decoding/counts.

Trusted native setup supplies one already leased Vault Arc, authenticated Owner,
selected Client producer and actual Runtime process ID. The Host retains the
kernel process object and checks same SID/logon session. Each accepted pipe
must have that actual kernel peer, checked before handshake/body dispatch and
again before response. The client separately retains/verifies the actual Host
process. Neither process name, clientInfo, principal, scope nor claimed PID in
a received packet can enroll anything. The legacy ProcessPeer surface marker
grants no Workspace capability and is never forwarded.

Runtime embedding retains RuntimeCaptureHost, optionally in an Arc. Its native
lock/withdrawal path must call revoke; unlocking requires fresh native setup.
serve_connection borrows the Host, atomically binds it once, and revokes on
every exit. Reusing the enrollment or reconnecting is refused. No client packet
opens roots, registers peers, changes scopes/policies, locks/unlocks or recovers
the Vault. Existing Agent readers and their native lifecycle remain independent.

## Frames and data

Reuse protected local Windows pipes, same-user ACLs, bounded deadlines and
u8-tag/u32le-length framing. Hello/welcome are at most 4096 bytes, calls at most
256 KiB and replies at most 1 MiB. Only hello then call frames are accepted;
Owner control tags refuse. Each connection accepts at most 256 calls and remains
bounded by its native producer lifetime. All IO/EOF ends the connection.

Hello exactly matches protocol, recordSchema 2, layout, native capability,
vaultId and compatibilityHash. The hash includes normalized capture/wire/common/
automation schema texts and framing descriptor. It conveys no authority.

Call is the closed object `{mode, request}`. Mode is write or preflight; request
is the closed native runtime_capture_request, retaining the original request ID.
Duplicate decoded keys, unknown fields, wrong major/capability and invalid
native data refuse before capture. Preflight and write use the same native
checks; preflight neither publishes nor reserves a key or source ID.

Reply is `{request_id, mode, capture, error}` with exactly one non-null result.
Capture is the actual native witnessed source result, with producer/scope,
observed snapshot and source references. The client validates closed types,
request/hash, Vault, source count/uniqueness, mode and flags. A prospective
preflight result is not a capture receipt. formation_scheduled remains false;
no job ID or formed memory is fabricated. Errors are content-free:
permission_denied, invalid_request, capacity, unavailable or outcome_unknown.
All packet/result shapes are in runtime-capture-pipe-v2.schema.json.

Known pre-send failures return NotSent. A send/read/malformed-response failure
returns OutcomeUnknown and retains the complete original call, including mode
and unchanged request key; later calls return Closed without sending. A known
outcome_unknown response also latches closure. The server retains the original
native request if its submitted write lacks a trustworthy response. There is
no automatic retry, replacement key, reconnection or recovery.

## Capability matrix and adoption

| Surface | Native Runtime capture | Agent reads/writes |
| --- | --- | --- |
| schema 1 / host-current-v2 | Refuse | Existing six tools retain original meaning |
| schema 2 / automation-host-v2/v3 | Refuse | Existing explicitly negotiated read capabilities |
| schema 2 / automation-segmented-host-v1 | Explicit new Runtime pipe only | Existing catalog reads; no Agent capture grant |
| Newer/unknown layout or Agent/Owner/System producer claim | Refuse | No implicit adoption |

Memory-only synthetic tests run the actual pipe with an independent owned
Runtime process and existing sole Vault. Runtime must later adopt the exact
pin/manifest and implement its reliable native hook. It must never treat an
Agent process as that Runtime producer or forward its own Workspace authority.
The shared initialization/System formation entry, durable live queue, ordered
conversation jobs, Agent memory_note and quote normalization remain separate
bindings. No mandatory per-memory Morii queue is introduced.
