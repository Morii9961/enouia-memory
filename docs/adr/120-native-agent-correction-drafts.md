# ADR-MEM-120 — Native Agent correction-report draft gate

2026-11-03. MCP-4 continuation, after the explicit MCP-3 note transport.

A correction report says a remembered statement was wrong. It is distinct from
an update to a previously true fact, an Owner edit and a forget request. The
Agent's interpretation is untrusted; an actual captured user statement must
support it. This slice establishes native preparation before durable intake,
System evaluation or a new advertised MCP tool.

Trusted native Owner setup grants a separate opaque AgentCorrectionPermit on
the initialized Owner carrier. It requires source-read permission and existing
source/policy/sensitivity/lifetime ceilings. Neither a read permit nor a note
permit can be passed as correction authority. Host generation invalidation,
expiry, revocation and current policy checks apply to each preparation.

The original request binds a request ID, exact TargetRef, exact captured source
revision, bounded quote hint and bounded correction interpretation. The current
target must be fully Agent-qualified, including its derivation/source closure.
An inaccessible target refuses without distinguishing unknown IDs; an exact
revision/hash mismatch on an accessible current target is stale. The correction
source must independently be currently authorized, native Runtime provenance,
at most 16 KiB, with actual User speaker and UserStatement evidence class.
Assistant text, manual assertions and fabricated UserConfirmation refuse.

The existing pinned quote locator maps hints to original text. The Vault checks
strict original scalar spans, source revision, speaker and evidence class under
the same current-state lock as target qualification. The draft binds original
evidence, exact target, Agent/delegation/snapshot, request hash and deterministic
report key. It grants no later write authority and means neither saved nor
corrected. Agent interpretation never becomes user evidence or Owner consent.

No chain delta, source, memory revision, queue entry, receipt, transport profile
or forget-hide behavior changes. Durable intake must independently repeat live
authorization and define original-key recovery after the target changes. System
revision evaluation follows separately; NativeMemoryOwner is not an Agent route.
See [native contract](../contracts/AGENT_CORRECTION_NATIVE_V1.md).
