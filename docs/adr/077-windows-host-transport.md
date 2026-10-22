# ADR-MEM-77 — Explicit Windows Host process and protected pipe prototype

Date: 2026-10-22. Scope: owner-authorized MV-8.1h, synthetic Windows prototype.

## Native enrollment and endpoint

The executable requires an explicit existing v2 root, native Host identity,
opaque pipe identifier, enrolled client PID and maximum HostSurface. It never
installs, autostarts, picks/creates/adopts a root, configures a Provider or falls
back to embedded writing. Native launch is the owner enrollment point; none
of these values comes from a page, imported document or handshake.

Enrollment retains a real process object with query/synchronize rights,
compares its Windows account SID and terminal-session ID with the Host, and
checks liveness. The server verifies the kernel-reported client PID against
that live object before reading metadata and before dispatch. The native
client independently verifies the pipe server against its enrolled live Host
object. PID reuse, an executable name, same-user access and JSON claims alone
do not supply a grant. This does not attest executable code, authenticate a
human or defend against compromise of the trusted native owner process.
Do not enroll an Agent/MCP process on the owner Workspace endpoint; it needs
a separately designed principal/scope protocol.

The pipe uses an explicit current-account DACL, rejects remote clients and
reserves its first instance. The prototype serves one enrolled broker process,
one connection at a time, reusing the reserved pipe over disconnect/reconnect.
It exits when that broker exits; `--once` bounds synthetic drills. Multiple
clients, grant enrollment UI/storage and native reference-shell/Runtime launch
policy remain separate work. It is not an installed background service.

These choices follow Microsoft's [pipe security/access rules](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights),
[CreateNamedPipeW](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-createnamedpipew),
[client PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid),
[server PID](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeserverprocessid)
and [retained process-object](https://learn.microsoft.com/en-us/windows/win32/procthread/process-handles-and-identifiers) APIs.

## Framing and lifecycle

Pipe v1 frames contain a one-byte tag, four-byte little-endian length, and exact
UTF-8 JSON body. Client tags are hello=1, call=2, native lock=3 and unlock=4;
server tags are welcome=129, reply=130 and acknowledgement=131. Roles cannot
overlap. Hello/welcome/control metadata is <=4 KiB, Workspace requests <=256 KiB,
replies <=1 MiB and acknowledgements <=64 bytes. Empty/oversized lengths are
refused before allocation/body reads. Serialization counts encoded bytes before
emitting a header. These caps can refuse otherwise-valid large Core envelopes;
there is no silent splitting, fallback or reapproval of a changed batch.

The exact framing descriptor now participates in the compatibility hash.
Earlier schema-only prototype clients need the matching new native pin. Core
Workspace envelopes/55 commands and their JSON schemas remain unchanged.
Every page envelope must travel as a fixed Call frame; a page may never choose
the native frame tag, enrolled identity or scope.

Ordinary hello creates a private connection handle. Native lock/unlock uses
the same closed ClientHello metadata and validates versions/hash/Vault even
without an ordinary session, allowing a freshly authenticated owner channel
to unlock a locked Host. Only the enrolled Workspace broker can invoke these
controls. Unlock then performs a fresh ordinary handshake; old handles never
revive. Actual Core calls and native controls retain ADR-MEM-76's fresh Host
checks, worker lifecycle and continuous Vault lease.

Connect/read/write use overlapped I/O and manual-reset events, with monotonic
per-operation/frame deadlines. `ERROR_PIPE_CONNECTED` is a successful connect
race. Timeout/error cancellation drains completion before releasing buffers,
events or OVERLAPPED storage; cancellation alone is not completion. No pipe
flush waits for a client to consume output. See [ConnectNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe),
[CancelIoEx](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-cancelioex)
and [GetOverlappedResult](https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-getoverlappedresult).
Safely draining cancellation can exceed the requested timeout; this is not
a hard real-time scheduling guarantee.

Native clients validate response shape, request identity, handshake Vault/
version/hash/surface and the live OS peer. Any uncertain I/O or invalid reply
poisons the channel; it cannot send/re-handshake again. Reconnect is explicit.
Neither side automatically repeats an application request or Provider send.
Existing original-key/unknown-invocation recovery remains authoritative.

## Boundaries

This is actual local process/pipe behavior with synthetic data and explicit
test enrollment, not reference-shell/Runtime adoption, human acceptance,
multi-client or MCP compatibility, large-history throughput, independent
restore, power-loss proof, cross-host fencing or cloud activation. Physical
purge/export remains fenced. See [verification](../validation/MV-8-windows-host-transport.md).
