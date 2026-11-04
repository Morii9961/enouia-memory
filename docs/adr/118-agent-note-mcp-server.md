# ADR-MEM-118 — Explicit MCP Agent-note server

2026-11-03. Continuation of ADR115–117, still synthetic-only.

The new explicit MCP server combines the five initialized read tools with
memory_note. It requires both an initialized Agent reader and a separately
delegated AgentNoteWriter for the same authenticated Agent and Vault. Each
delegation retains its own scope; their hashes need not match. Constructing a
server from packet-supplied identity or an old read permit is impossible.

Existing pipe-5 tools and capability stay unchanged. This slice adds the protocol
server and closed note packet/output schema only. Actual OS peer enrollment and
a distinct pipe-6 handshake are a subsequent transport slice; no existing pipe
can reach the new tool by changing its name or arguments.

memory_note accepts a bounded original native note request, wrapped in schema 2,
kind agent_note_request and capability memory-agent-note-1. It submits through
the existing native intake, retaining real Agent/source provenance and current
authority. Its success means only hint_saved and returns the original independent
receipt. It neither captures text nor claims formation or Owner approval.

Native OutcomeUnknown is not converted to unavailable or a fabricated MCP tool
result. The server closes its session, retains the complete original MCP Value,
and exposes an uncertainty marker. The transport must disconnect without writing
a Reply. A future native Client can then retain the entire sent packet and block
further calls, using the existing transport-uncertainty mechanism. A serialized
MCP message never authorizes replay; fresh native setup and the exact original
domain key remain required after recovery.

Known refusals are bounded content-free errors with retryable=false. Connection
polling checks both native lifecycles, note uncertainty and current policy
references without taking the filesystem writer lock. Submission still owns
CURRENT, exact source, policy and deletion checks.

No real client configuration, model, private Vault, Runtime pin or production
activation is part of this slice. See [server contract](../contracts/AGENT_NOTE_MCP_V1.md).
