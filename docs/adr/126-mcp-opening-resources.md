# ADR-MEM-126 — Scoped MCP opening prompt and resources

2026-11-04. Morii explicitly authorized MCP-5 synthetic development after MCP-4.
Other existing boundaries remain. This slice is read-only on the initialized
Owner carrier; there is no capture hook, model call or Owner action.

## Decision

Add an explicit server with the existing five initialized read tools, one
`memory_start` prompt and bounded resources. Reuse the same independently
authorized QualifiedAgentReader and its exact leased Vault/Agent, current
policy, source closure, sensitivity, generation, expiry and deletion checks.
Resources and prompts are projections of that read authority, never a new
permit, System reader, Owner operation or substitute for source-read permission.

MCP remains 2025-11-25. Announce tools, resources and prompts in the new profile
only. No subscription/list-change notifications, completions, session capture,
automatic startup, background work, new write tools or model invocation.
Old pipe-5 and the separate report profiles retain their behavior.

Fixed guide: enouia-memory://guide. Parameterized resources:
enouia-memory://memory/{memory_id} and enouia-memory://recall/{request_id}.
Validate exact typed logical IDs; never resolve filesystem/network URIs, paths,
query strings or arbitrary schemes. Listing returns static descriptors only,
not memory IDs, titles or counts. Unknown/inaccessible resources return the
same content-free not-found error. Memory resources use current qualified detail
without source text; the native source-read gate still governs metadata. Recall
resources return only the actual last response cached on that same native
reader/request, after fresh authorization/snapshot checks; never recompute it.

The user-controlled memory_start prompt accepts string request_id and at, with
optional topic. at is an explicit ranking clock, not event time or permission.
Use Current intent for a nonblank topic, Browse for an empty topic; no implicit
conditions, historical window or invented date. Bound selection to min(4,
profile.max_items) and min(16384, profile.max_bytes), using the existing pure
qualified recall/evaluator. The result includes actual trace, coverage and
attributed records, wrapped as data in a standard MCP text prompt. MCP role=user
does not authenticate a captured UserStatement, Owner consent or new evidence.
It changes only the existing ephemeral reader context cache, not durable data.

Use new memory-agent-pipe-10, profile memory-agent-initialized-context-1 and
context capability memory-agent-context-1. Exact hello binds the selected Vault,
initialized read fingerprint, context schemas and static descriptors/guide.
Native OS enrollment pairs the Agent and MemoryRead / optional SourceRead;
no packet supplies identity or scope. Explicit automation-context-v1 stdio
attach mode opens no Vault and configures no real client. Every exit revokes
the native reader and disconnects the borrowed pipe. Transport uncertainty keeps
the full original MCP Value, poisons sends and never retries/reconnects. A native
OutcomeUnknown live gate likewise emits no result and retains the first Value.

No chain/schema-2 delta, worker, Runtime pin/repository, real model/Provider,
private data, external host hook, deployment or new MV stage is added.
See [contract](../contracts/MCP_OPENING_RESOURCES_V1.md).

## Pinned protocol sources

- [Resources 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/resources):
  resources capability, list/read/templates, custom URIs, authorization and -32002.
- [Prompts 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/prompts):
  prompts capability, list/get, string arguments and structured messages.

These describe protocol behavior, not authorization or model/client adoption.

## 2026-11-05 — Complete response contracts before adoption

Keep mcp-opening-v1 and schema(startup, output) as the existing parsed content /
argument contracts. Add mcp-opening-wire-v1 for the actual closed JSON-RPC
success replies of resources/list, resources/templates/list, resources/read,
prompts/list and prompts/get. Match the pinned 2025-11-25 standard fields;
restrict this profile to its advertised static catalogs and one text item.
Expose response_schema(method) for those five methods, bind all five schemas in
the exact pipe-10 compatibility hash and validate actual replies against them.
Existing errors and initialized tools retain their contracts. Parsing/checking
the text's embedded memory payload remains a separate test; a JSON string in a
valid MCP envelope is not proof of valid memory content or authorization.

This closes contract coverage in the unadopted draft. No new method, authority,
capture, model or native business effect is added. The new exact fingerprint
requires matching endpoint revisions; do not reconnect or fall back on mismatch.
