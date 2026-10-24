# ADR-MEM-80 — Restricted MCP stdio boundary

Date: 2026-10-23. Status: accepted for the owner-authorized MV-8.2 local synthetic continuation.

Implement MV-8.2a as a transport-neutral library in the Host crate. Pin MCP
2025-11-25, UTF-8 newline-delimited JSON-RPC, explicit initialize/initialized,
ping, tools/list and tools/call only. Bound input, output and session requests;
reject duplicate JSON fields. No HTTP, SDK dependency, client installation,
autostart, root discovery, Provider send or production registration is added.

The six tools are exactly INTERFACES §2: memory_search, memory_read,
memory_source, context_get, memory_propose and session_checkpoint. Their inputs
are the existing full Memory IPC v1 request envelopes, including caller-retained
requestId and write idempotencyKey. The operation must match the selected tool.
Advertised schemas are derived from memory-v1 and bundled common definitions,
with no external references. memory_update is an unadvertised legacy alias and
retains the existing revise/supersede-only validation. Owner review, identity,
delete, import, arbitrary Workspace calls and native lifecycle controls are absent.

A trusted embedding adapter supplies a non-serializable Agent grant and a typed
AgentPort. ClientInfo, tool packets and imported instructions never enroll a
principal or scope. Only the five scopes required by these tools are accepted;
source:read is independent of memory:read. Revocation and session end invalidate
the grant. The port receives the native principal, parsed request and typed
arguments, and must independently enforce current record/project/sensitivity/
destination policy, Vault authority, provenance and durable keyed receipts.

The library's grant constructor is not OS authentication. Current owner/window
Host sessions MUST NOT be used as an AgentPort by relabeling requests. This slice
provides only fake-port and synthetic stdio evidence; an authenticated scoped
Host/domain adapter remains a separate MV-8.2 implementation slice. No executable
in this change opens a Vault or exposes the current owner Workspace to MCP.

Validate backend response kind, requestId and existing Memory IPC response rules.
An uncertain call or malformed backend response disconnects, revokes admission,
and returns the original packet/key with a nonretryable outcome_unknown result.
Never replay, reconnect, generate replacement keys or turn a proposal/checkpoint
into Canonical approval. stdout failure also ends the session; stderr contains
no request bodies, roots, credentials or raw exceptions. A new session requires
fresh native admission. A synchronous port is not advertised as cancellable work.

Primary sources: [stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
[tools and errors](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
This pins a supported revision, not a claim that it is the newest revision.
Real-client compatibility, native Host binding, Runtime adoption, private data,
cloud and migration remain outside this slice.
