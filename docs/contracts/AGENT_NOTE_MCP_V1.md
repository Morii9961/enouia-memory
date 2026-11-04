# Agent note MCP protocol v1

2026-11-03. [ADR118](../adr/118-agent-note-mcp-server.md).

Explicit new server only; old initialized pipe-5 stays five-tool/read-only.
Native construction requires live initialized reader and independently created
note writer with identical principal/Vault. Per-tool native scopes stay separate.

memory_note input: schema_version 2, kind agent_note_request, capability
memory-agent-note-1, request containing request_id, exact source revision,
quote_hint and note. Native bounds and strict quote/source authority are those
of AGENT_NOTE_INTAKE_V1. No identity, PID, grant, scope or Owner field is accepted.

Output: schema_version 2, kind agent_note_response, capability memory-agent-note-1,
request_id, receipt (original AgentNoteReceipt or null), error (null or a closed
content-free code with retryable=false). Success contains state hint_saved only.
No note/source body, memory identifier, global sequence or native scope hash is
echoed. Original independent receipt identity survives replay and reopen.

The five read tools keep their existing initialized packet/output schemas. The
new server shares protocol initialization, global request-ID uniqueness and
bounded session count, and separately advertises memory_note as a native write.
It never forwards to an Owner method or upgrades a read grant.

OutcomeUnknown: no tool response; session closes, outcome_unknown() is true and
original_unknown() retains the complete original MCP Value. The transport must
actually disconnect before writing any Reply, preserving client uncertainty and
the original packet. No automatic resend/reconnect, replacement key or mandatory
human review queue is permitted. This slice does not yet supply pipe-6 or stdio.

The explicit [pipe-6 transport](AGENT_NOTE_PIPE_V1.md) supplies that next slice;
it disconnects on unknown before Reply and preserves the full Value in stdio.
