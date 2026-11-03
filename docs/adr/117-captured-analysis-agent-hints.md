# ADR-MEM-117 — Optional Agent hints in captured analysis

2026-11-03. Continuation of [ADR116](116-durable-agent-note-intake.md).

Saving a hint must not become a second capture or automatic re-analysis route.
The ordinary captured worker may include currently eligible hints when it first
dispatches analysis for an already queued source. Analysis remains optional and
untrusted, and strict source evidence still decides which statements can form.

Native analysis v3 explicitly passes a bounded slice of AgentNoteRecord values
to CaptureAnalyzer::start. Existing adapters must update their signatures rather
than inherit a compatibility default. No real analyzer, egress or MCP write is
activated by adopting this local interface.

Under the same Vault lock that selects the source/current policy, choose notes
whose exact source revision and current policy reference match. Policy changes,
deletion or another source cannot lend hints. Select at most eight in stable
note-key order and trim the suffix until their canonical serialized array fits
64 KiB. These are optional inputs, not a promise of complete hint coverage.

Persist the selected note keys in the dispatch journal before calling start.
Those immutable keys identify the exact saved Agent records that influenced
this attempt. Later notes do not change a pending analysis, create another
handoff, reopen terminal work or trigger a human queue. Paused still proves no
handoff and may select fresh current input on a later attempt. Unknown/panic/
restart remains terminal without resending.

The existing journal keeps version 1. note_keys defaults to empty and is omitted
when empty, so prior dispatch/recovery identities stay intact. A journal with
nonempty note keys already belongs to a root containing ADR116 Agent-note deltas;
old binaries cannot decode that root. Do not create a new journal filename or
lose old job/receipt identities to upgrade this field. Producer wire statuses
do not expose note text, principals or keys.

The evaluator continues to resolve every produced quote against the original
source and actual speaker/class. Agent interpretations are not Owner evidence,
automatic truth or permission to send data elsewhere. Runtime-only capture and
independent foreground/cancellation/unknown-result behavior remain unchanged.
