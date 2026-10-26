# Automation V2 Agent reads and local MCP

2026-10-26. [ADR 095](../adr/095-native-agent-automation-reads.md),
[ADR 096](../adr/096-versioned-automation-agent-mcp.md).
Schema: [agent-read-v2](../../contracts/next/agent-read-v2.schema.json).

## Native authority

`AutomationHost::agent_reader` is trusted native registration, not an RPC. The
native Owner must already be authenticated independently. It supplies an Agent
principal and `AgentReadScope`: exact subject/project, 1..128 unique source IDs
within current automation policy, a sensitivity ceiling, independent source-read
permission, and exclusive valid_until within five minutes/current policy validity.
These values never come from clientInfo, tool arguments or a model. Registration
is collection-scoped; it does not create a per-memory owner review queue.

The sealed non-Clone/nonserializable permit binds a random scope hash, exact
current policy reference and live Vault instance. The borrowed reader binds the
Host lock generation. New Host/Vault instances, unlock or re-registration cannot
revive an old reader. Revocation, expiry, clock regression and policy replacement
refuse further reads. Independent OS enrollment retains a live process handle,
checks the protected pipe peer's kernel identity, and consumes one connection.
Its native principal must equal the reader's Agent principal; memory:read is
mandatory, source:read is independent, and write/context scopes are refused.
A Grant constructor is not OS authentication.

Current primary/witness, policy, source revision, deletion and sensitivity gates
apply before projection, counts and ranking. All original/context/counter-source
references and derivation/reversion ancestors must fit the delegated scope.
Otherwise withhold the entire item, including its history and edges. Missing and
inaccessible selected identities are indistinguishable. Source metadata/text
requires independent source-read permission; evidence references remain provenance.
No System reader, Owner Workspace operation or second writer is exposed.

## Explicit compatibility and capabilities

| Reader/writer | schema-1 / host-current-v2 | schema-2 / automation-host-v2 |
| --- | --- | --- |
| Legacy six tools / memory-agent-pipe-1 | Existing reads, pending proposals and temporary checkpoints | Refused before business decode/query/count; source-pin upgrades do not add support |
| This five-tool Agent / memory-agent-pipe-2 | Incompatible; no fallback or conversion | Explicit memory-agent-read-1 scoped reads only |
| Core-7 System native reader | Unsupported by the automation reader | memory-native-read-1 stays System-only, never an Agent credential |
| Core automation writer | No route through this endpoint | Existing native System/formation authority only; no Agent write port |
| New Agent capture/correction/session tools | No aliases into old proposal semantics | Not delivered; MCP-3/4/5 and field contract prerequisites remain |

The parallel UI record contract at `24a03e02bdf18094d60bc4d3268d380bd1b669a1`
([ADR 093](https://github.com/Morii9961/enouia-memory/blob/24a03e02bdf18094d60bc4d3268d380bd1b669a1/docs/adr/093-ui-v2-record-layout.md))
introduces **automation-host-v3**, still schema 2. This MCP-2 implementation does
**not** accept v3 or the parallel segmented layout: only automation-host-v2 is
supported. Extending the schema enum/pin alone must not enable new-root reads.
A follow-up Agent adapter must preserve its record fields and native projection
gates; no legacy or System reader may substitute for it.

The pipe-2 Hello exactly matches protocol=memory-agent-pipe-2, recordSchema=2,
layout=automation-host-v2, capability=memory-agent-read-1, mcpVersion=2025-11-25,
memoryVersion=2, vaultId and a compatibility hash of the descriptor and nine
normalized schema texts. Unsupported roots/protocols refuse at this handshake;
unsupported business packets refuse before dispatch. A matching major alone is
insufficient. The legacy six-tool catalogue and contracts are unchanged.

## Tool packets and results

MCP stays pinned to 2025-11-25. All five tools advertise closed bundled
inputSchema and outputSchema with local references only, plus readOnlyHint=true,
destructiveHint=false, idempotentHint=true and openWorldHint=false. These hints
are descriptive; authorization still comes from native registration.

Each argument envelope has schema_version=2, kind=agent_read_request,
capability=memory-agent-read-1, request_id, operation matching the tool name, and
closed operation-specific arguments. Principal, scope, PID, System capability,
Workspace forwarding and unknown/unfrozen filters cannot be supplied.
Results have kind=agent_read_response, schema_version=2, capability, request_id,
operation, the native principal/scope_hash, data, error and coverage. The
structuredContent and serialized JSON text agree. A known tool refusal conforms
to outputSchema with data=null and an error with retryable=false. Invalid
protocol packets return JSON-RPC errors before business dispatch. Missing and
inaccessible read/source/related results are data=null without inventing records.

| Tool | Supported data and bounds |
| --- | --- |
| memory_search | Literal all-term text (2048 bytes / 32 terms), form/adoption/state/project, event/valid/learned/source_captured time axes, optional window and explicit unknown-time handling. Only authorized counts; stable pages 1..32 items, 2..256 KiB. Cursor binds current snapshot, query/profile/sensitivity and random grant scope hash. |
| memory_read | Qualified current item, actual stored formation/receipts, bounded ordered history/source views and actual local provenance neighbors. Optional expected snapshot and explicit source-text request. No source permission means no source metadata/body; an explicit text request is refused. |
| memory_source | Selected authorized source revision and UTF-8 excerpt, byte offsets/hash/full length, complete and next_start; excerpt limit 1..8192 bytes. Does not expose filesystem paths. |
| memory_related | Actual derivation/reversion provenance links within scope. Fact and suggested edges are explicitly unavailable, not inferred or counted as full graph support. |
| memory_recall | Core-6 pure selection/rendering over current authorized data, complete trace, rendered_context, qualified selected items and coverage. Historical ranking time never overrides current policy/deletion/validity. No attention reinforcement or saved experiment. |

Native `AgentReader::context_trace(request_id)` returns the exact last computed
AgentRecall (same trace, rendered text and items) under that same reader/grant.
It does not rerank a simulation or attest delivery/use by the Agent. New snapshot,
policy, expiry, revocation or lock refuses a stale trace. The single-response
cache is ephemeral. Runtime must explicitly adopt this native port to render it;
this slice does not change Runtime or prove its rendered UI.

Reads allocate no operation, receipt or durable experiment and perform no business
writes. Per-connection search/read/source/related/recall call counters are separate
ephemeral metadata, not attention, billing or Activity/Usage input. Stored receipt
meaning remains unchanged. Attributed statements are subject statements, not
verified facts; machine adoption is not Owner approval, and staged uncertainty
continues automatic reassessment. Time, validity, knowledge and adoption remain
separate axes.

## Local stdio entry

A native operator supplies an existing protected pipe and independently enrolls
the bridge process into that Host. The optional standard MCP stdio edge is:

```text
enouia-memory-mcp --protocol automation-v2 --pipe <pipe-hash> --host-pid <live-host-pid> --vault-id <vault-id>
```

These four options identify the endpoint/expected peer; they confer no principal
or scope. This mode opens no Vault, starts no System Host and grants no authority.
`--protocol legacy-v1` explicitly attaches to an enrolled v1 endpoint; the existing
six-option legacy launcher retains its behavior. Mixed attach/launch arguments
are rejected. No actual MCP application configuration is installed or modified.
The runnable synthetic executable test creates the existing Host, enrolls the
actual bridge PID and exercises initialize/tools/list/recall over real stdio/pipe.

EOF closes the connection. Lock/revocation/expiry/process exit invalidate it;
reconnection requires explicit native re-enrollment. Transport uncertainty retains
the original packet; no automatic retry, reconnect or new key. IO failure on stdio
is a protocol/transport error (retryable=false, originalRequest), never a fabricated
tool success or transaction receipt. A known per-tool scope refusal keeps a still
valid connection usable; a dead native reader closes it.

## Coverage and deferred fields

The Runtime UI v2 handoff is a requirement, not a record contract. This endpoint
exposes the current Core-7 fields and declares coverage on every result. Title,
content_lang, entities, time precision/bound/tz-basis, extended formation note/
prompt/import/live metadata, topics and fact/suggested edges remain unavailable
until the frozen UI record contract and subsequent projections are explicitly
adopted. The parallel field ADR is now available at the exact revision above;
this delivery does not yet incorporate it. Unsupported filters are rejected, never
silently ignored or represented as complete empty coverage. Exact current source
and formation fields, knowledge/adoption and separate event/valid/learned/source
capture times are preserved.

MCP-3 must use the frozen record fields and the same System/live formation pipeline
as initialization. Agent hints remain hints and Agent speech never becomes native
Owner confirmation. Capture hooks/default timezone and forget-hide policy require
Morii's decisions at their respective slices. No migration, real Provider or
embedding, private Vault, Activity/Usage, Runtime edit, remote opening or deployment
is part of this delivery. See [synthetic evidence](../validation/MCP-2-automation-agent-reads.md).
