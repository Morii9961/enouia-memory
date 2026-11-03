# Agent correction-report native draft v1

2026-11-03. [ADR-MEM-120](../adr/120-native-agent-correction-drafts.md).

Capability `memory-agent-correction-native-1`, initialized segmented Owner roots.
Trusted native setup authenticates the Owner and independently delegates an
AgentCorrectionPermit; packet fields never select authority, principal or scope.

```text
QualifiedAgentHost::correction_reporter(owner, agent, scope) -> AgentCorrectionReporter
AgentCorrectionReporter::prepare(request) -> AgentCorrectionDraft
AgentCorrectionReporter::revoke()
```

AgentCorrectionRequest is closed: request_id, target (exact TargetRef), source
(exact SourceRevisionRef), quote_hint and correction. quote_hint is nonempty,
at most 500 Unicode scalars / 2000 UTF-8 bytes; correction is nonempty, at most
2000 scalars / 8192 UTF-8 bytes. The source is at most 16384 bytes. Only an
independently authorized current native Runtime User/UserStatement source is
accepted. Attachments, other turns and arbitrary Agent speech are not evidence.

The target must be the current Agent-qualified revision with the exact hash;
its complete policy/source/derivation closure must be within this delegation.
Missing or inaccessible targets are denied; an accessible target with a changed
revision/hash is stale. The source must also be current and separately allowed.
An authorized target does not grant access to a correction source or vice versa.

The Host's pinned locator normalizes quote hints; the Vault independently checks
the exact original quote/scalar span, source reference, speaker and evidence class.
The returned draft has schema_version 2, capability, request_id, request_hash,
report_key, principal, scope_hash, snapshot, authorized_at, target, evidence and
correction. report_key is digest of (capability, vault ID, Agent, request ID).
request_hash covers the complete request. Repeated preparation is deterministic
data preparation, not durable idempotency or a promise to accept later submission.

The lifecycle read gate stays held through preparation. Old generations after
lock/unlock, stale policy, revocation, expiry, foreign native instances and deleted
sources/targets refuse. Existing note/read permits cannot satisfy this API.

Drafts contain untrusted Agent interpretation alongside actual user evidence;
they are not Owner confirmation, saved-report receipts or corrected memories.
This API writes no state, schedules no work and advertises no MCP tool. Durable
intake must take the original request, reauthorize at publication and preserve
original packet/key uncertainty. It must also define receipt replay after a
System revision changes the current target. Forget-hide remains a separate
owner decision under MCP_AGENT_READ_V2, with no hide effect in this slice.
