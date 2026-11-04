# ADR-MEM-122 — System consumption of saved correction reports

2026-11-03. MCP-4 continuation of [ADR121](121-durable-agent-correction-reports.md).

A saved report must reach memory automatically, without an Owner review queue
and without letting the Agent's interpretation become truth or permission.

## Decision

The existing captured formation worker also consumes saved `agent_correction`
records, one bounded step at a time, on the same leased Vault, worker lease,
private dispatch journal, timeout, stop token and unknown-result rules. A
separate native port keeps CaptureAnalyzer v3 unchanged:

```text
CorrectionAnalyzer::start(report, target: &CorrectionTarget, source, stop)
    -> Result<PendingCaptureAnalysis, CaptureAnalysis>
CapturedFormation::step_corrections(analyzer, stop) -> bool
```

Selection runs under the Vault lock in report-key order. The job key is
digest("agent-correction-job-1", report_key); a journal entry makes it terminal
or in progress, so each report is analyzed at most once. If the target memory or
evidence source is deleted, or the report's policy is no longer active or no
longer covers the exact source revision, the job closes as Rejected without
analysis. Otherwise the analyzer receives the report, the evidence source and
the target's **current** revision (content, title, interpretation, dimensions,
sensitivity); the System model decides whether the user statement corrects it.

The dispatch intent and report_key are durable before start. Paused proves no
handoff and removes the entry; Unknown, panic, timeout and restart after dispatch
are terminal AnalysisUnknown with no resend. NoMemory means no change. More than
one statement, oversize output, a statement whose quote does not resolve in the
evidence source, or a change other than `correction` is refused.

An accepted statement becomes an ordinary System AdmissionInput bound to the
current target revision dispatched (result revision + 1), sensitivity at least
the target's, evidence the exact user quote. The existing evaluator and
`apply_batch` decide: policy revise scope, stale target, same kind, new evidence
and every other rule apply unchanged. A target that moves between dispatch and
application is refused, not retargeted. Admission keeps full revision history;
the Agent text never becomes content or evidence.

Capture and correction dispatch never overlap; each waits while the other has
pending analysis. The Host worker loop may drive both ports; capture behavior,
producer status and pipe-2 meanings are unchanged.

## Compatibility

Journal version 1 at the same path gains optional `report_key`, omitted when
absent. A journal containing it belongs to a root with ADR121 deltas, which old
binaries already refuse. No chain format, MCP tool, Owner action, forget-hide,
real analyzer, model, egress or Runtime pin is added.

See [consumption contract](../contracts/AGENT_CORRECTION_CONSUMPTION_V1.md).
