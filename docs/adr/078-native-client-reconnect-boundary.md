# ADR-MEM-78 — Native client reconnect boundary

Date: 2026-10-22. Status: accepted for the owner-authorized MV-8 continuation.

ADR-MEM-77 supplies a real native connection, but a shell also queues work
across lock, disconnect and reconnect. A queued page request must not silently
move to a new connection. An uncertain reply must not trigger another send.

Add an opt-in native `Broker` to the Host adapter. It owns one authenticated
pipe client, its maximum native surface and a private connection ticket. A
shell captures that ticket when accepting work, before queuing a worker. The
ticket is an in-process identity, not serializable page input or authentication.
Every replacement, handshake, lock, unlock, disconnect or uncertain exchange
revokes earlier tickets, including tickets from another Broker instance.

The caller supplies the actual native window surface independently of the
page packet. Both that surface and the broker's enrolled maximum scope must
permit the exact validated envelope. Ordinary calls always use the Call tag;
they cannot select native controls. Native lock/unlock require Workspace
scope and an explicit native action. A successful page `vault_lock` also
invalidates queued work and records the locked state. Root switching and page
unlock remain unavailable through HostService.

Expose local Disconnected / AwaitingHandshake / Ready / Locked states without
Vault contents, endpoint paths or native handles. Ready means the latest
handshake succeeded, not a live health or unexpired-session guarantee. A
transport failure drops the channel and records Disconnected. Return an
explicit uncertain-call error once a valid call reaches the transport; its
Core outcome may already be durable. The broker retains no request queue,
never resends a packet, opens no embedded writer and spawns no process.
Reconnection, a fresh handshake and any exact-key replay are separate caller
actions. Native control failure likewise requires an explicit new connection.

The reference acceptance harness uses actual synthetic Windows pipes/Host
processes to check stale queued work, per-window restrictions, lock/reconnect
and response loss after a real durable commit. Runtime and the Tauri shell
must still adopt the adapter explicitly. No product UI, enrollment policy,
multi-client support, Provider, MCP or real-data activation is implied.
