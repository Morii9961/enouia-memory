# ADR-MEM-92 — Native Agent enrollment and local MCP pipe

Date: 2026-10-25. Accepted for Morii's MCP-Resume-1 synthetic implementation.

The trusted native launcher fixes one Agent principal, five-scope ceiling and
explicit live client process before accepting any wire input. Retain the process
object; require matching Windows SID/terminal session and kernel pipe peer PID,
before handshake and each dispatch. Reuse the protected local-only first-instance
pipe and bounded overlapped IO. Same-user access, clientInfo, packet PID/scope,
executable names and ordinary Agent text supply no authority. This is native
operator enrollment, not executable attestation or human authentication.

The Agent endpoint is distinct from the owner endpoint. It borrows an existing
HostService and its sole Vault writer; only the six existing MCP tools reach
HostPort. No Workspace forwarding, owner lifecycle commands, System read port,
new Context selection or automatic Canonical write is exposed. The executable's
explicit Agent mode opens one HostService; an embedding native launcher may pass
its already bound service. It never opens a second writer for an existing Host.

Freeze [the local contract](../contracts/MCP_NATIVE_AGENT_V1.md) before coding.
Each enrollment is consumed by one connection, with a five-minute monotonic
ceiling in addition to the existing gate TTL/instance/serial checks. Disconnect,
expiry, revocation, lock/unlock or Host exit cannot revive old admission. Native
revocation shares the grant flag; re-enrollment is an explicit launcher action.
No automatic retry/reconnect/renewal occurs. A failed exchange after dispatch is
outcome_unknown with the unchanged original message, tool packet and key. Only
a real legal Host transaction supplies a success receipt.

Support schema-1 host-current-v2 only. Exact Agent capability and compatibility
hash are checked before MCP dispatch. Existing descriptor/CURRENT gates refuse
schema-2 automation-host-v2 before business decode/query/count. Core-7 System
native read is not an Agent reader. V2 Agent adaptation is explicitly deferred.
Old proposals remain pending and checkpoints provisional, attributed to Agent;
neither is Morii's self-report or Owner confirmation. Staged automatic memory
remains automatic reassessment, without a mandatory owner queue.

Validate only new temporary synthetic roots and owned children. No private
Vault, credentials, real clients/Provider, Runtime modification, network listener,
migration or deployment. See [actual evidence](../validation/MCP-native-agent-transport.md).
