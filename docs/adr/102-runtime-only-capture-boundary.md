# ADR-MEM-102 — Native Runtime-only daily capture

2026-10-27. MCP-3. Morii selected Runtime hooks only and Asia/Shanghai for
missing source timezone, with explicit default-source provenance.

Use a separate sealed native Runtime producer delegation and retained OS peer.
Its principal is Client, not an Agent/Owner/System supplied by a packet. Native
setup binds one existing leased segmented Vault, exact current policy, subject,
project, selected source-ID pool, sensitivity, platform/account alias, default
timezone and a maximum five-minute lifetime. No wildcard/source-scope expansion
or Owner confirmation is added. No Workspace/System read port is forwarded.

Capture raw turns with original IDs, parent/branch/sequence, role, text, source
time, language/model and bounded attachment metadata. Source IDs are chosen by
the backend from the delegated pool. User turns remain user statements;
assistant/tool/system turns cannot become user confirmation. Preserve unknown
event dates. Keep original_timezone null when absent and record resolved timezone
and owner_default origin in the typed runtime.capture source extension. That
extension grants no authority and never changes a claim's date basis.

Preflight runs the same current gates and validation without publication,
reservation or durable counters. Actual source capture uses the existing
System delta transaction and real witness. Same-key changed work conflicts;
unknown storage/transport outcomes retain the exact original packet/key and
never automatically resend or reconnect.

The first checkpoint is a native building block. Do not expose conversation_capture
as an operational Agent tool or claim queued formation until the Runtime-only
local transport and durable per-conversation live pipeline are connected and
tested. All text, identities and processes used for verification are synthetic.
No private initialization, provider, Runtime edit or client configuration.

Checkpoint 2 binds that same producer to memory-runtime-capture-pipe-1,
with exact layout/capability/transitive-schema handshake. One enrollment binds
at most one connection and is revoked on exit. Native control may retain the
Host in an Arc and revoke an active connection; no lock/unlock or registration
packet exists. The Runtime client retains the full original call on uncertainty.
This source-only transport still schedules no formation and is not an Agent
MCP capture tool. It never forwards the legacy Workspace surface marker.
