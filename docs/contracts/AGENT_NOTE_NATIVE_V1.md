# Agent note native draft v1

2026-11-03. [ADR-MEM-115](../adr/115-native-agent-note-drafts.md).

Native capability `memory-agent-note-native-1`, Owner segmented roots only.
QualifiedAgentHost must explicitly adopt the initialized carrier profile.

```text
QualifiedAgentHost::note_writer(owner, agent, scope) -> AgentNoteWriter
AgentNoteWriter::prepare(request) -> AgentNoteDraft
AgentNoteWriter::revoke()
```

The native request contains request_id, exact SourceRevisionRef, quote_hint and
note. Nonempty quote_hint is limited to 500 scalars; note is nonempty and limited
to 2000 scalars and 8192 UTF-8 bytes. The captured source is limited to 16384 UTF-8
bytes. Only native Runtime capture provenance is accepted. Attachments and other
conversation turns are not read. No input field grants authority.

The draft carries schema_version 2, capability, request_id, request_hash,
note_key, principal, scope_hash, snapshot, authorized_at, exact QuotedEvidence and
note. note_key is digest of (capability, vault ID, Agent principal, request ID).
request_hash covers the entire request. The locator handles normalization, and
QuotedEvidence::resolves still verifies the original quote/scalar span and actual
speaker/class. Drafts are untrusted Agent hints, not independent evidence or
Owner confirmations. They grant no right to commit a prepared draft later.

Every prepare checks the current source/policy/deletion state and live native
delegation. Revoke, lock, unlock generation replacement, policy revision change,
expiry or another instance refuses. An Agent scope with source_read false cannot
receive a note permit. A read permit cannot be passed to this API.

This is a read-only preparation API, without a saved-hint receipt, durable
idempotency, formation effect, new MCP tool or transport. Captured formation runs
independently of whether any Agent supplies a hint. Later submission must repeat
the owning boundary checks and preserve original packet/key uncertainty rules.

Native durable submission is specified separately in [intake v1](AGENT_NOTE_INTAKE_V1.md).
It accepts the original request and current delegation rather than a draft.
