# ADR-MEM-96 — Versioned automation-root MCP reads

2026-10-26. Accepted for Morii's MCP-2 continuation, following Runtime UI v2
03-MCP_REQUIREMENTS.md as requirements, not as a record schema.

Expose memory_search/read/source/related/recall on a separate Agent pipe v2,
record schema 2 / automation-host-v2 / memory-agent-read-1. Pipe v1 and its six
legacy tools are unchanged and still refuse automation roots before queries.
The native operator binds one OS-enrolled Agent process, one borrowed scoped
AgentReader and one fixed attention profile. Grant principals must match;
memory:read is mandatory, source:read remains independent. No clientInfo or
packet field grants scope. No System port, Workspace forwarding or second writer.

MCP stays pinned to 2025-11-25. Tool input is a closed schema-2 Agent request
(kind/capability/request_id/operation/arguments), not legacy Memory IPC. Each tool
declares bundled inputSchema/outputSchema, readOnlyHint=true,
destructiveHint=false, idempotentHint=true, openWorldHint=false. Structured output
and serialized text agree. Output schema covers both success and known refusal.
See [official versioned tools specification](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).

Reuse ADR95 Agent authorization before counts/ranking/text/history/edges. Source
excerpts are UTF-8 bounded with explicit partial coverage. Related returns only
actual typed provenance edges; unavailable fact/suggestion edges are declared.
Search supports existing form/adoption/state/project and explicit event/valid/
learned/source-capture axes. Unsupported title/entity/topic/precision filters
refuse, with unavailable fields declared until the new field contract is adopted.

Recall uses the same Core-6 pure selector/rendering over the Agent-authorized
set. Keep the complete trace and qualified original items. Native Context reads
the exact cached computed response for that request, not a reranked simulation;
new snapshots, revocation or policy changes refuse stale traces. This ephemeral
single-response cache does not attest delivery, persist an experiment or touch
attention. Tool call counters are per connection and separate from memory state.

Validate all five tools through a real protected pipe with an independent owned
Agent process, repeated-read no-reinforcement and independent output-schema
checks. No personal data, real client configuration, Provider/embedding, Runtime
change, remote opening or deployment. Capture/correction/session tools wait for
their own slices and explicit adoption of the parallel field contract.

Late verification found UI ADR 093 at 24a03e02bdf18094d60bc4d3268d380bd1b669a1
introduces automation-host-v3. This endpoint intentionally remains HostV2-only;
MCP-2b must adopt that root and its complete fields before claiming UI v2 parity.
The parallel segmented-read work is not modified or treated as merged here.
