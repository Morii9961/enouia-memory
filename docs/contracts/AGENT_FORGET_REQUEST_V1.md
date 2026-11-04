# Agent forget-request recording v1

2026-11-04. [ADR125](../adr/125-agent-forget-request-recording.md).

## Native recording

QualifiedAgentHost::forget_requester(owner, Agent, scope) independently grants
AgentForgetRequester/AgentForgetPermit on an initialized leased Owner Vault.
The original AgentForgetRequest is closed: request_id, exact target (TargetRef),
exact source (SourceRevisionRef), quote_hint and forget (untrusted Agent text).
No identity, scope, Owner approval or effect settings are accepted from packets.
Bounds match report intake: nonblank quote_hint at most 500 scalars/2000 bytes;
nonblank forget at most 2000 scalars/8192 bytes; source at most 16384 bytes and
actual current native Runtime User/UserStatement provenance. Native quote
location and strict original spans remain authoritative. The interpretation is
never content, evidence or consent to an effect.

submit(original request) rechecks live generation/permit/current policy and the
entire Agent-qualified target/source closure, then the same snapshot under the
existing publication lock. request_key=digest(memory-agent-forget-native-1,
Vault ID, Agent, request ID); request_hash covers the full original request.
AgentForgetRecord stores request key, request ID/hash, principal/scope, exact
target, current source policy, original QuotedEvidence and untrusted forget.
The agent_forget_request delta is written by the native System on the existing
Owner chain; there is no second authoritative journal.

AgentForgetReceipt contains request_key, request_id, request_hash, original
target, committed_in and state=forget_request_saved only. It means recorded,
not forgotten, hidden, deleted, approved, queued or promised for processing.
No source body, Agent text, result memory, global sequence or native scope is
echoed. Recall, process state, source/memory bytes, Owner marks, suppression and
worker dispatch remain unchanged by the request. The chain head records storage.

Current authorization always precedes receipt lookup. Same key/hash replays
without a commit; changed hash conflicts. Saved old target must be in currently
authorized history; an unsaved old target is Stale. Delete/revoke/expiry/lock or
policy/scope exclusion refuses before replay. Independently at most 32 requests
per target, with existing root ceilings; saved keys still replay at capacity.
OutcomeUnknown latches the exact request and revokes its permit. All later calls
on that requester refuse. Explicit chain recovery plus fresh native setup may
repeat only that original request/key; no automatic retry/reconnect/replacement.

## Explicit MCP and pipe

Closed input: schema_version=2, kind=agent_forget_request,
capability=memory-agent-forget-1, request=the original native request.
Closed output: schema_version=2, kind=agent_forget_response, matching capability,
request_id, receipt and error. Success is the original forget_request_saved
receipt and null error. Known refusal has null receipt and a content-free closed
error code with retryable=false (same bounded codes as ADR123). Unknown is never
a tool result. Tool annotation is a nondestructive, idempotent native write.

The server advertises five initialized reads plus memory_forget_request and
requires independent live reader/requester, same native Agent/exact leased
Vault. Native OS enrollment requires MemoryRead + SourceRead but does not grant
request authority. Protocol memory-agent-pipe-9, recordSchema=2,
layout=automation-segmented-owner-v1,
profile=memory-agent-initialized-forget-requests-1. Exact Hello binds descriptor,
Vault, initialized read fingerprint and bundled local-only wire schemas.
Old profiles, changed hashes, extra authority fields and wrong peers refuse
before business dispatch. Both native lifecycles are checked before/after each
frame and call. Every exit revokes both and actually disconnects.

Native publication unknown: no Reply, full original MCP Value retained, Client
poisoned, sends blocked. Explicit recovery/fresh enrollment may recover the same
request/key. automation-forget-requests-v1 is an explicit stdio attach mode;
error.data.originalRequest and output-failure retention preserve the full parsed
MCP Value. Original formatting/bytes are not promised. It opens no Vault, grants
no enrollment and changes no real configuration.

There is no consumer, Owner task queue, model request or forget-hide behavior.
Any future effect needs Morii's separate scope/duration/restore/revocation
decision and an independent implementation/authorization. This contract is not
that decision. New closed deltas must never be opened by older binaries.
