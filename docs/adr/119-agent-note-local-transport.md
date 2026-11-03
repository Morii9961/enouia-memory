# ADR-MEM-119 — Agent note local transport

2026-11-03. Continuation of [ADR118](118-agent-note-mcp-server.md).

memory-agent-pipe-6 explicitly negotiates the initialized read + independently
delegated note profile on one leased Owner Vault. It reuses protected pipes,
kernel peer verification, native Enrollment lifetime/revocation, frame limits
and strict JSON parsing. MemoryRead + SourceRead enrollment pairs the native
Agent identity; it never substitutes for the independent NotePermit. Old
MemoryPropose or pipe-5 cannot grant note authority.

The exact hello binds the new descriptor, selected Vault, existing initialized
read compatibility hash and reachable note input/output schemas. Unknown or old
profiles/layouts/hash/extra fields refuse before business dispatch. Both native
delegations are checked before each frame and business call. Every exit revokes
them and actually disconnects the borrowed server pipe.

On native OutcomeUnknown, disconnect before writing a Reply. The reused Client
exchange retains the complete original MCP Value, poisons the connection and
never resends/reconnects. A fresh native enrollment after explicit recovery may
replay only the original request/key. A new stdio path returns the full MCP Value
in originalRequest for this profile; old stdio profiles retain their existing
arguments-only output. Output failure after completion also retains the original
full message. No fabricated tool result represents transport uncertainty.

The existing launcher gains explicit --protocol automation-notes-v1 attach mode
with the same trusted pipe/Host PID/Vault parameters. It does not discover/open
a Vault, create a grant, edit any client configuration or activate a real Agent.
All acceptance uses synthetic roots and owned child processes only.
