# ADR-MEM-127 — Read-only session_start opening recipe

2026-11-05. Continue the authorized MCP-5 opening/resources route. Earlier MCP
handoffs list session_start as undelivered; ADR126 supplied a prompt, resources
and five existing read tools but no tools/call entry for the opening recipe.

Add session_start only to the opt-in pipe-10 context profile. It uses the same
initialized QualifiedAgentReader and bounded recall recipe as memory_start:
typed original request ID, explicit ranking clock, optional topic, Current or
Browse, min(4, profile.max_items) / min(16384, profile.max_bytes). Model clients
can discover/invoke an ordinary read tool; no prompt/manual step is required.

This is opening context preparation, not creation of a durable Session. No
session ID, capture, checkpoint, Provider, reinforcement, job, receipt, Owner
operation or user-review queue is produced. No SessionPropose/ContextRead scope
or Note/Correction permit substitutes for the initialized read authority.

Closed input capability memory-agent-session-start-1, schema_version=2,
kind=agent_session_start_request and request={request_id,at,topic?}. Closed
success kind=agent_session_start_response returns the original request_id and
context: the exact tagged recall payload already used by resources/prompts.
Return normal MCP text plus structuredContent using the existing tool_result
format. Errors/lifecycle/complete original Value retention use the context
server's existing behavior; no native write or new lifecycle implementation.

The six-tool context catalog, session tool descriptor/schemas and guide enter
the exact pipe-10 fingerprint. This unadopted draft needs matching endpoints;
old pipe-5, pipe-6 and report profiles remain unchanged. Mismatch never falls
back, reconnects or replays. Guide discovery names the optional opening tool.

The pinned [MCP tools 2025-11-25](https://modelcontextprotocol.io/specification/2025-11-25/server/tools)
defines tool discovery/invocation and structured output schemas. Protocol shape
and read-only annotations grant no authority or assurance about model behavior.
No real client/configuration, Runtime pin, private root or new MV is activated.
See [contract](../contracts/AGENT_SESSION_START_MCP_V1.md).
