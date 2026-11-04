# Captured analysis native v3

2026-11-03. [ADR117](../adr/117-captured-analysis-agent-hints.md).

```text
CaptureAnalyzer::start(job, source, notes: &[AgentNoteRecord], stop)
    -> Result<PendingCaptureAnalysis, CaptureAnalysis>
```

An explicit compile-time update for native adapters. start/poll/drop remain
bounded, nonblocking and cancelable. The dedicated worker, foreground priority,
existing pending timeout and unknown-result rules retain their meanings. No
blocking adapter or default method silently discards hints.

Before a first dispatch, the same locked lookup authorizes the source/current
policy and selects optional saved notes. Exact source revision and current policy
reference must match. At most eight notes in stable note-key order are selected;
the suffix is trimmed until their canonical serialized array fits 65536 bytes.
The analysis does not promise complete coverage of all saved notes. Actual
speaker/class, Agent provenance and each original source quote remain intact.

The private dispatch journal records those immutable note_keys before start.
They bind the saved hint inputs for audit. A later saved note does not change an
in-flight analysis, restart terminal work or cause another model request. Paused
still proves no handoff; a later attempt may select fresh current hints. Unknown,
panic and restart after dispatch remain terminal without resend. Notes never
become independent source evidence or Owner confirmation.

Journal version 1 stays at its original path. note_keys defaults to empty and is
omitted when empty, preserving old jobs and receipts. Nonempty keys require a
root with ADR116 note deltas, which old binaries already cannot decode. Producer
status and existing wire enums do not expose keys, note text or Agent identities.

No real analyzer/model/Provider, egress grant, MCP write tool, Runtime pin or
private root adoption is activated by this native interface.
