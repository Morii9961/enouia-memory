# Agent note pipe v1

2026-11-03. [ADR119](../adr/119-agent-note-local-transport.md).

Protocol memory-agent-pipe-6, recordSchema 2, automation-segmented-owner-v1,
profile memory-agent-initialized-notes-1; five initialized read tools plus
memory_note. The note wire capability stays memory-agent-note-1. Native setup
must provide the same leased Vault instance/Agent for reader and independent
note writer, with actual retained OS peer Enrollment. No packet grants a scope.

Hello is an exact closed native assertion: descriptor fields, Vault ID and
compatibilityHash binding the initialized read fingerprint and bundled note
input/output schemas. Old pipe-5/layout/hash/extra fields refuse before dispatch.
Every request uses existing bounded Call/Reply framing and unique-key parsing.
Every exit revokes the paired delegations and disconnects the actual pipe.

Known results use AGENT_NOTE_MCP_V1. Native unknown publication produces no Reply:
disconnect and let the existing Client report OutcomeUnknown, retain the complete
MCP Value and block further sends. No automatic reconnect/retry/replacement key.
After explicit recovery and fresh native setup, the original domain request/key
can recover its independent receipt. A successful save never means formation.

The explicit automation-notes-v1 launcher attach mode supplies a stdio bridge.
Its error data.originalRequest is the complete original MCP Value, unlike older
read-only bridges' arguments-only output. Stdout failure also retains the full
packet locally and closes the channel. Original JSON formatting/bytes are not
claimed; the complete parsed MCP Value is retained.

No real MCP configuration, Owner root selection, model, provider, private data,
Runtime pin or production activation is supplied by this transport.
