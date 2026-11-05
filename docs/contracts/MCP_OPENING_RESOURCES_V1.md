# MCP opening and resources v1

2026-11-04. [ADR126](../adr/126-mcp-opening-resources.md).

Native Server::new(reader, attention_profile) requires a live initialized
QualifiedAgentReader; it grants no authority. Same native reader backs all five
existing tools, memory_start and resources. Global MCP request-ID/count and
initialization gates are shared. Existing tool input/output envelopes stay
memory-agent-initialized-read-1. New content capability is memory-agent-context-1.

## Resources

resources/list returns the fixed enouia-memory://guide descriptor.
resources/templates/list returns exactly two application/json templates:
enouia-memory://memory/{memory_id} and enouia-memory://recall/{request_id}.
These single-page catalogs accept only optional _meta; there is no issued cursor,
subscription or notification. No scope-dependent identity/title/count is listed.

resources/read accepts only uri and optional _meta. guide returns the frozen
capability/attribution guide. memory/{id} parses an exact MemoryId and calls
current native detail_page with no expected pin/history cursor, no source text
and 65536 bytes. The existing qualified history/metadata and truncation rules
apply. recall/{id} parses RequestId and calls context_trace on this exact reader;
it returns only that reader's actual last response with the same request ID,
scope hash and current snapshot, never a new selection.

The parsed content payload is closed: schema_version=2,
kind=agent_context_resource, capability=memory-agent-context-1, uri,
resource_type=guide|memory|recall, data. Data is the exact guide or existing
initialized read_data/recall_data projection. Return it in standard
contents=[{uri,mimeType:application/json,text:serialized payload}]. No filesystem,
HTTP, decoded paths, query strings, fragments or arbitrary templates are resolved.
Unknown/unauthorized IDs and unsupported URIs use -32002/resource_not_found
without echoing content. Malformed packets use -32602; stale native context uses
-32000/stale_scope. Known invalid/denied/unavailable failures stay content-free.

## Opening prompt

prompts/list advertises memory_start with required string request_id and at,
optional string topic. prompts/get accepts only name, arguments and optional
_meta. name must be memory_start. arguments is closed: typed RequestId, Timestamp
and optional topic (at most 2048 UTF-8 bytes, 2048 scalars). Unknown fields,
principal/scope or effect settings refuse. Empty topic means Browse; nonblank
means Current. No conditions/event window. At most four items / 16384 rendered
bytes, further narrowed by the native attention profile. The current native
clock still owns validity and authorization even when ranking at another time.

Call native recall once with the original typed request ID; return one standard
user/text message containing the explicit data/attribution preamble and the
same recall resource payload. MCP prompt role does not create a Runtime user
turn or authenticate UserStatement/Owner consent. The actual result is available
through recall/{request_id} only while the existing ephemeral cache remains
current; a later recall replaces it. No file, receipt, job, reinforcement,
checkpoint, model call or mandatory human work is produced. There is no automatic
client startup/configuration or promise that an Agent used the returned context.

If new authorized formation changes the snapshot, an old recall URI returns
known stale_scope without recomputing it. While authority is still live, an
explicit new memory_start read may run on the same connection with its own read
request ID. This is not automatic replay/reconnection, and does not apply to
OutcomeUnknown or a revoked grant. A later recall replaces the old cache entry.

## Lifecycle and transport

Protocol memory-agent-pipe-10, recordSchema=2,
layout=automation-segmented-owner-v1,
profile=memory-agent-initialized-context-1. Exact hello binds initialized read
compatibility, context schemas and static guide/catalog/prompt descriptors.
Native OS enrollment requires MemoryRead, plus SourceRead when the native reader
has it. Broader SourceRead enrollment never expands a narrower native reader;
the reader still controls source metadata/text. No other scopes are accepted,
and unsupported write/context scopes never substitute. The same leased
Vault/Agent/current native grant is checked before/after each frame and call.
Every exit revokes the reader and actually disconnects the borrowed pipe.

Initialize declares tools:{}, resources:{}, prompts:{}; no subscribe/listChanged
or completions. Older profiles never gain resources/prompts by changing a tool
name. Explicit automation-context-v1 launcher attach mode takes the existing
trusted pipe, expected Host PID and Vault ID; it opens no root or registration.
Stdio preserves complete originalRequest on transport/output uncertainty.
OutcomeUnknown from the native live gate returns no result, closes and retains
the first complete MCP Value; subsequent calls cannot replace it. Client sends
are poisoned and there is no automatic retry, reconnect or replacement key.

The supported MCP resources/prompts signatures are those of the pinned official
2025-11-25 specification linked from ADR126. This profile has no write, real
model/Provider, private-root access, Runtime configuration or product acceptance.

## Actual success envelopes (2026-11-05)

mcp-opening-v1 / schema(startup, output) continue to describe parsed content
payloads and method arguments. mcp-opening-wire-v1 / response_schema(method)
describe complete JSON-RPC success Values for the five new methods:
resources/list, resources/templates/list, resources/read, prompts/list, prompts/get.
Each requires jsonrpc=2.0, the original admitted string/safe-integer id and the
method-specific result, with no error or unknown envelope fields. Catalogs have
exactly the advertised static entries and issue no nextCursor. prompts/get has
one user/text message; resources/read has one application/json text content
with a supported logical URI. This is the exact emitted profile subset, not a
schema for every possible MCP server or client extension.

Validate the full Value, then independently parse/validate the embedded JSON
payload. The response schema cannot authenticate a user role, authorize an ID,
make Agent text evidence or validate JSON hidden inside a string. Native gates
still own those decisions. All five success-envelope schemas enter the pipe-10
compatibility fingerprint; mismatch refuses before business dispatch, with no
automatic fallback/reconnect. Existing error/tool envelopes are unchanged.
