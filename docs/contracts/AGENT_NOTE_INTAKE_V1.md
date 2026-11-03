# Agent note intake v1

2026-11-03. [ADR116](../adr/116-durable-agent-note-intake.md).

Native Owner-root API, using ADR115's independently granted AgentNotePermit.

```text
AgentNoteWriter::submit(original_request) -> AgentNoteReceipt
```

The original request, quote resolution and bounds remain those of
[native preparation](AGENT_NOTE_NATIVE_V1.md). Submission repeats preparation;
it does not accept an AgentNoteDraft as input. The same snapshot must remain
current when publication starts. Agent provenance is set from the permit.

AgentNoteRecord contains note_key, request_id, request_hash, principal,
scope_hash, exact current PolicyRef, QuotedEvidence and note. The record is an
untrusted Agent interpretation with original source evidence, not a new source,
Owner assertion or machine admission decision. The existing System actor writes
the `agent_note` delta. Chain replay validates the record and current source,
policy, key uniqueness, layout and capacity before applying it.

AgentNoteReceipt contains note_key, request_id, request_hash, committed_in and
state `hint_saved`. It contains no source body, memory ID, global sequence or
claim about consumption/formation. Matching replay returns the same receipt;
changed request hash under a saved principal/request key conflicts. A fresh
delegation may replay only while it still authorizes the exact source.

At most 32 notes per source and MAX_SEGMENTED_REVISIONS notes per root. Capacity
is rejected before publication. A moved snapshot is Stale; callers may re-read
and explicitly resubmit the unchanged original request. Busy is not acceptance.
OutcomeUnknown retains the original request/key, closes the Vault instance and
requires explicit recovery and fresh native setup before replay.

No new hint journal, automatic re-analysis, memory publication, external call,
page command, MCP tool or pipe capability is part of this native intake slice.
Older binaries cannot decode the new closed delta and must refuse such roots.
