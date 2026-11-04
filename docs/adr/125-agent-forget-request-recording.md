# ADR-MEM-125 — Record Agent forget requests without forgetting

2026-11-04. Authorized request-only MCP-4 route after supersede reports.

## Decision

memory_forget_request records an untrusted request against one exact memory
target and an independently authorized captured Runtime User/UserStatement quote.
It performs no forgetting, hiding, deletion, suppression, Owner action or model
analysis. There is no mandatory Owner queue. Scope, duration, restoration and
revocation rules for forget-hide remain unchosen by Morii; this slice supplies
none of them and cannot be used as an approval for later effects.

Native Owner setup independently grants AgentForgetPermit on the initialized
Owner carrier, with the same current target/source closure, policy, sensitivity,
source-read and bounded lifetime gates as ADR120/121. Existing read, note,
correction, supersede and MemoryPropose permissions do not grant this authority.
AgentForgetRequester::submit takes the original request only, never an Owner
method, serialized draft or packet-supplied identity. The Agent's bounded forget
interpretation is data, not user evidence or authorization to forget.

Save a closed agent_forget_request delta on the witnessed Owner chain, using
memory-agent-forget-native-1 and its independent (Vault, Agent, request ID) key
space. The independent receipt has state=forget_request_saved only. Current
authorization precedes receipt replay. New requests need the exact current
target; saved original keys may replay through authorized target history after
a System revision. Changed payload conflicts. Independently cap 32 requests per
target and retain existing root ceilings. Unknown publication latches the exact
original request, closes authority and requires explicit recovery/fresh setup
to replay only that original; no automatic resend or new key.

Expose five initialized reads plus memory_forget_request through explicit
memory-agent-pipe-9, profile memory-agent-initialized-forget-requests-1, wire
capability memory-agent-forget-1, and automation-forget-requests-v1 stdio attach.
Pair a native reader and independent requester for the same Agent and exact
leased Vault. OS MemoryRead + SourceRead enrollment pairs identity only. Exact
closed Hello binds descriptor/Vault/read fingerprint/reachable wire schemas.
Old profiles and permits retain their meanings. Check both native lifecycles
before/after frames and calls; every exit revokes both and disconnects.

On OutcomeUnknown emit no MCP tool result or Reply; retain the complete original
parsed MCP Value, actually disconnect and poison the Client. Stdio preserves the
full originalRequest and retains it on output failure. Only explicit recovery
and fresh enrollment may recover the independent original-key receipt.

No dispatch journal/worker change. The schema-2 delta is additive and closed;
older binaries must not open a root containing it. Read fingerprints that bind
catalog-v2 change without expanding read authority. No Runtime/client config,
private data, real model/Provider/egress, forget-hide, deployment or MCP-5.
See [contract](../contracts/AGENT_FORGET_REQUEST_V1.md).
