# Agent session_start MCP v1

2026-11-05. ADR127; MCP-5 read-only opening recipe on pipe-10 only.

tools/list includes the original five initialized tools and session_start.
Its annotations are readOnlyHint=true, destructiveHint=false,
idempotentHint=true, openWorldHint=false. These are descriptive, not grants.

tools/call params is closed: name=session_start, arguments, optional object
_meta. arguments is closed: schema_version=2, kind=agent_session_start_request,
capability=memory-agent-session-start-1, request. request is the existing closed
opening input: typed request_id, millisecond UTC at, optional topic (<=2048 UTF-8
bytes and <=2048 scalars). No principal, Vault, permission, source, session ID,
capture, model, effect or persistence setting is accepted. _meta cannot grant.

The same initialized reader owns every native authorization/snapshot check.
Call its opening recall once: Current for nonblank topic, Browse otherwise,
no event window/conditions, min(4, profile.max_items) items and
min(16384, profile.max_bytes) rendered bytes. at is a ranking clock, never
permission or authenticated event time. Native current validity still applies.

Success structuredContent is closed: schema_version=2,
kind=agent_session_start_response, capability=memory-agent-session-start-1,
request_id and context. context is exactly the existing recall resource payload
(memory-agent-context-1), including URI and actual native recall data. Text
content serializes that same structuredContent. No state called session_saved,
session_created or session_started is returned. No durable Session is created.

The same reader's actual recall URI retrieves this context while current;
successful tool/prompt recalls replace it, invalid reads do not overwrite it,
and stale context is never recomputed by resource lookup. All RPC IDs share the
context session's initialized/duplicate/count gates. Existing five tool schemas
and meanings are unchanged. session_start is optional for all other reads.

Malformed arguments use the context server's existing JSON-RPC invalid-argument
errors; native known failures use its content-free fault mapping. Unknown or
RecoveryRequired emits no result, retains the first complete MCP Value, closes,
disconnects and revokes at the native boundary. No retry/reconnect/rekey.

No capture or checkpoint, no source/Agent interpretation promotion, model call,
receipt, job, Owner method or mandatory queue. Use only an independently enrolled
MemoryRead reader (SourceRead remains optional) on the same leased Vault/Agent.
No ContextRead, NotePermit, CorrectionPermit or MemoryPropose inference.

The pipe-10 context fingerprint binds the additional descriptor/input/output
schemas and guide field opening_tool=session_start. Match endpoint revisions;
no old profile upgrade or fallback. No real MCP config or Runtime adoption.
