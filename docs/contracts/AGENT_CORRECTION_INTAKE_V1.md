# Agent correction-report intake v1

2026-11-03. [ADR121](../adr/121-durable-agent-correction-reports.md).

Native Owner-root API on ADR120's independently granted AgentCorrectionPermit.

```text
AgentCorrectionReporter::submit(original_request) -> AgentCorrectionReceipt
AgentCorrectionReporter::original_unknown() -> Option<AgentCorrectionRequest>
```

The request, bounds, quote resolution and target/source rules are those of
[native preparation](AGENT_CORRECTION_NATIVE_V1.md). Submission never accepts
an AgentCorrectionDraft. Agent provenance comes from the permit.

AgentCorrectionRecord: report_key, request_id, request_hash, principal,
scope_hash, target (exact TargetRef), policy (current PolicyRef covering the
evidence source), evidence (QuotedEvidence, supports `content`) and correction.
It is an untrusted Agent interpretation beside original user evidence, not a
source, Owner assertion, admission decision or memory revision. The System actor
writes the `agent_correction` delta. Replay validates key derivation, Agent
principal, shape bounds, key uniqueness, capacity, current exact target, current
Runtime User/UserStatement source, policy reference/activity and quote resolution.

AgentCorrectionReceipt: report_key, request_id, request_hash, target,
committed_in and state `report_saved`. It contains no source body, memory
result, global sequence or claim of consumption/correction.

| Case | Result |
| --- | --- |
| Lock, revoke, expiry, policy change, deleted/unauthorized target or source | Locked/Denied before any receipt |
| Saved key, same request hash, target current or in authorized history | Original receipt, no commit |
| Saved key, changed request hash | Conflict |
| Unsaved key, accessible target no longer current | Stale |
| Unsaved key, exact current target and strict evidence | Commit, receipt |
| Snapshot moved between authorization and publication | Stale; re-read and resubmit unchanged |
| 32 reports on the target memory, or root ceiling | Capacity before publication |
| Publication uncertain | OutcomeUnknown; original request latched, permit revoked |

After OutcomeUnknown every call on that reporter returns OutcomeUnknown. Recover
the chain explicitly, set up a fresh native reporter and repeat the exact request.
Busy is not acceptance. No consumer, recall change, page command, MCP tool or
pipe capability is part of this slice; older binaries must refuse such roots.
