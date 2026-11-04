# Agent correction-report consumption v1

2026-11-03. [ADR122](../adr/122-system-correction-consumption.md).

Native System port on initialized Owner roots, beside
[captured analysis v3](CAPTURE_ANALYSIS_V3.md).

```text
CorrectionAnalyzer::start(report: &AgentCorrectionRecord,
                          target: &CorrectionTarget,
                          source: &CapturedSource,
                          stop: &CaptureStopToken)
    -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis>
CapturedFormation::step_corrections(analyzer, stop) -> bool
```

CorrectionTarget: target (current TargetRef), content, title, interpretation,
dimensions and sensitivity of the current revision. start/poll/drop are bounded,
nonblocking and cancelable; no Vault lock is held while they run.

| Outcome | Job state | Effect |
| --- | --- | --- |
| Target/source deleted, policy inactive or not covering the source | Rejected | No analysis |
| Paused (before handoff) | entry removed | May dispatch later |
| Unknown, panic, timeout, stop or restart after dispatch | AnalysisUnknown | Never resent |
| NoMemory or no statements | NoMemory | No change |
| More than one statement, over 256 KiB, unresolvable quote, change not `correction` | Rejected | refused counted |
| One statement | Forming → Formed/Rejected | Evaluator + `apply_batch` decide |

The seed is the ordinary formation seed with target = the dispatched current
revision, result = same memory at revision + 1, sensitivity = max(source,
target), content/evidence = the exact user quote from the report's evidence
source. Moved targets, policy revise scope, kind change and reused evidence are
refused by existing rules. Receipts are ordinary MemoryReceipts. Because native
prepare rebinds the head itself, its Stale for a target-bound seed means the
target moved or vanished; the worker refuses that work instead of retrying.

Private journal version 1, same path: optional `report_key`, omitted if absent;
job_key = digest("agent-correction-job-1", report_key). Each report is
dispatched at most once. Capture and correction dispatch never overlap.
No MCP tool, Owner action, recall hiding, real analyzer or egress is part of v1.
