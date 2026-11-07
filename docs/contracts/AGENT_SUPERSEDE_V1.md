# Agent supersede report v1

2026-11-04. [ADR124](../adr/124-agent-supersede-reports.md).

## Native intake

QualifiedAgentHost::supersede_reporter(owner, Agent, scope) independently creates
AgentSupersedeReporter/AgentSupersedePermit. SourceRead and the initialized Owner
carrier are mandatory. No existing permit or serialized draft is accepted as
authority. The principal, exact Vault instance, generation, current policy,
five-minute expiry and fully qualified target/source closure are native gates.

AgentSupersedeRequest is closed: request_id, target (exact TargetRef), source
(exact SourceRevisionRef), quote_hint and supersede. Bounds: quote 1..500 Unicode
scalars / at most 2000 UTF-8 bytes; interpretation 1..2000 scalars / at most
8192 bytes, both nonblank; captured Runtime User/UserStatement source at most
16384 bytes. Host locator and strict Vault quote resolution preserve original
spans. Agent text cannot become evidence, content or Owner confirmation.

prepare returns a data-only draft. submit takes the original request, repeats
authorization and uses the same snapshot under the publication lock. The
report_key is digest(memory-agent-supersede-native-1, Vault, Agent, request ID);
request_hash covers the full request. Save AgentSupersedeRecord with report key,
request ID/hash, native principal/scope, target, current source PolicyRef,
QuotedEvidence and untrusted supersede. AgentSupersedeReceipt contains report
key, request ID/hash, original target, committed_in and
state=supersede_report_saved only. No promise of acceptance, update or review.

Same key/hash replays the original receipt; changed hash conflicts. After target
revision the exact old target must still be in currently authorized history;
an unsaved old target is Stale. Deletion/revocation/expiry/policy/scope changes
refuse first. At most 32 supersede reports per target and the existing root
ceiling; old receipts still replay at capacity. Native OutcomeUnknown latches
the exact original request and revokes the permit; all further calls refuse.
Explicit chain recovery and fresh native setup may replay only that original.

## System consumption

SupersedeAnalyzer::start(&AgentSupersedeRecord, &CorrectionTarget,
&CapturedSource, &CaptureStopToken) uses the existing bounded cancelable
PendingCaptureAnalysis outcome port. CorrectionTarget is data about the current
target revision, not correction authority. CapturedFormation::step_supersedes
uses the same lease/journal/timeout. The independent job key is
digest(agent-supersede-job-1, report_key); private Job.supersede_key is optional
and omitted for old jobs. Every report is dispatched at most once. Journal loss
has the same reanalysis limitation as existing captured/correction jobs.

Deleted target/source or inactive/noncovering policy: Rejected, no dispatch.
Paused removes the intent for later dispatch. Unknown/panic/timeout/stop/restart:
terminal AnalysisUnknown, no resend. NoMemory/empty statements: no change.
Multiple statements, oversized output, unresolvable quote or change other than
update: Rejected. One accepted user quote becomes content/evidence of a normal
target-bound System update, revision + 1 and sensitivity at least the target's;
the existing evaluator and apply_batch decide. A moved target is refused.
No mandatory human queue or Agent-authored content is introduced.

RuntimeCaptureHost::start_worker_with_reports(capture, correction, supersede,
timeout) explicitly opts into all three native ports. Existing worker entry
points retain their previous set of consumers. Dispatch never overlaps.

## MCP and pipe

Closed input schema_version=2, kind=agent_supersede_request,
capability=memory-agent-supersede-1, request=original native request. Closed
output has matching version/capability, kind=agent_supersede_response, request_id,
receipt and error. Success returns the independent receipt only. Known refusal
returns null receipt and closed error code/retryable=false, as ADR123. Agent text,
source body, native principal/scope and result memory are never echoed.

Five initialized reads plus memory_supersede_report; no other writes. The server
requires independent initialized reader and supersede reporter with the same
Agent and exact leased Vault. Protocol memory-agent-pipe-8, recordSchema=2,
layout=automation-segmented-owner-v1,
profile=memory-agent-initialized-supersedes-1. OS MemoryRead + SourceRead enrollment
pairs identity, never grants report authority. Hello exactly binds descriptor,
Vault, initialized fingerprint and bundled local-only input/output schemas.
Old/forged profiles, hashes, identities, scopes and peers refuse before dispatch.
Both delegations are checked before/after frames and before business calls.

Unknown publication: no tool result/Reply, close and actually disconnect,
revoke both grants, retain complete original parsed MCP Value and block sends.
Explicit recovery plus fresh native setup may replay the original domain key;
no automatic retry/reconnect/new key. Explicit automation-supersedes-v1 stdio
attach mode preserves full originalRequest and output-failure retention, opens
no Vault, grants no enrollment and edits no real configuration.
