# ADR-MEM-75 — Versioned native Host session gate

Date: 2026-10-22. Scope: owner-authorized MV-8.1f, library before transport.

Host clients need connection-bound authority rather than a page-supplied
principal, surface or reusable session token. The new `enouia-memory-host`
crate provides native admission, exact handshake compatibility and a bounded
session/lifecycle gate. It has no listener, executable, actual OS peer
authentication, Workspace attachment, Runtime switch or MCP endpoint yet.

`ClientHello` is a closed <=4 KiB document: schema/protocol/Workspace versions,
selected Vault ID and compatibility hash. `ServerHello` adds native primary,
process-instance identity, granted HostSurface and expiry. Neither document
contains a principal claim, secret or resumable authorization token. Raw
Host parsing rejects duplicate decoded fields, unknown keys and budget
overflow. Scope serialization is additive; Workspace IPC remains 55 commands.

The Host adapter's compatibility hash covers normalized exact Workspace/common/authority/
hello/welcome schema text, protocol versions and Host v2 layout. It proves
matching schema bytes, not executable equality or peer identity. Pin executable
revisions separately and bump protocol versions for semantic incompatibility.
Schema text is embedded only in the Host adapter. The Contract crate keeps
pure types/versions and has no repository-file inclusion dependency.

`NativeAdmission::from_authenticated_owner` is a trusted native assertion,
not an authenticator: the adapter must first authenticate and authorize the
owner process/window and derive its allowed surface. Never construct it from
JSON claims, imported text, an executable name or same-user access alone.
MCP Agent admission requires separate scoped design, not this owner path.

Each connection owns a private, nonserializable/noncloneable handle bound to
one gate instance. At most 64 live connections last five minutes without
automatic renewal. Dropped/expired connections can be reclaimed without
evicting live peers. Reconnect requires fresh native admission and handshake;
old handles cannot cross a restart or be revived after unlock/revocation.

A mutex serializes synchronous dispatch, admission and revocation. Scope and
the ordinary Workspace parser run before Core dispatch. `vault_lock` revokes
all connections before invoking Core, even if Core subsequently fails. Clock
regression/invalid time revokes and locks; native unlock clears all old
connections. A panicking native backend poisons the gate and makes it
unavailable, with no embedded fallback.

The dispatch closure must recheck actual Host authority and serialize the
complete Core/lifecycle action without calling back into the gate. Async
workers still need existing Core cancellation/lock checks; synchronous
admission grants no authority for later work. Listener framing, bounded raw
reads, OS peer identity, owner enrollment, native unlock, full disconnect/
reconnect and actual process exclusion remain transport integration gates.
See [synthetic evidence](../validation/MV-8-native-session-gate.md).
