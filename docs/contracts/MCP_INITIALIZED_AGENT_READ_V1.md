# Initialized-root MCP adoption boundary v1

2026-10-28, ADR-MEM-105 checkpoint 1. Refusal is implemented first; a new
initialized-root Agent reader remains unadvertised.

| Root / carrier | Existing Agent access | Existing native Runtime capture |
| --- | --- | --- |
| schema 1 / host-current-v2 | Original six-tool contract | Refuse |
| schema 2 / automation-host-v2/v3 | Existing explicit read capabilities | Refuse |
| schema 2 / segmented-host-v1, original RuntimeEvent carrier | memory-agent-catalog-read-1 | memory-runtime-capture-native-1 / pipe-1 |
| schema 2 / automation-segmented-owner-v1 | Refuse before Agent snapshot/query/count | Refuse before source/policy transaction planning |
| Initializer import/attachment extension | Not adopted by this checkpoint | Not adopted |
| Unknown layout, forged Owner/System/Client identity or scope | No authority | No authority |

The trusted native embedding already holds the sole leased Vault. Existing
SegmentedAgentHost::from_authenticated_owner checks the exact old layout before
snapshot inspection; its separate sealed Agent permit keeps all current policy,
subject/project/source/sensitivity/instance/lifecycle checks. Runtime capture
checks the exact old layout before issuing a Client permit. Nothing received
over a pipe can call these factories or choose a principal. No old tool forwards
native Owner gesture/consent methods. Ordinary Agent text remains Agent material.

The Core now supports the published Owner root and qualified native catalog-4 /
process-v2 DTOs. Parsing those DTOs is not MCP compatibility. The existing Agent
v1 projection keeps its old root and wire meaning; do not silently replace its
selection with Owner v4 or emit hidden global receipt/turn sequence fields from
a new root. Later adaptation needs a distinct negotiated capability and output
schema, current Agent authorization before every projection and count, matching
native Context trace and synthetic actual-pipe acceptance.

Tests use newly created Owner roots, real legal genesis and current policies.
Where payload corruption is used to prove early format refusal, only an owned
temporary synthetic root is changed; no successful receipt is fabricated. Root
format refusal must not create a writer lease or change business files. Existing
RuntimeEvent roots and capture/preflight semantics remain regression-tested.

No initializer/live job, new Agent write tool, new reader advertisement, Runtime
hook/pin/UI, real Provider/embedding, private root, deployment or main merge is
claimed by this checkpoint.

## Checkpoint 2 native qualified page

memory-agent-qualified-native-read-1 explicitly supports only
automation-segmented-owner-v1. Old factories still reject this root, and this
new factory rejects older layouts. Native setup supplies the authenticated
Owner, Agent principal, exact active policies, subject/project/source ceiling,
sensitivity, source-read bit and lifetime. The sealed permit binds the instance,
profile and current exact policy references; no packet supplies identity or
scope. This native constructor itself does not enroll an OS process.

The native page has the Agent capability/principal/scope, catalog-4 selection,
exact scoped counts, complete revision fields/Owner marks and bounded paging.
It emits no System-capability envelope, raw Owner record or global receipt
sequence. Its data is produced by the same pure page projector used by native
catalog reads, after independent Agent qualification. Current visible targets
and this delegation's scope hash bind selection/cursors; passing another
delegation's cursor refuses even if its current visible rows happen to match.

Owner overlays are recomputed from the currently permitted memory/source/entity
view. Merge destinations require their own complete delegated evidence and
each memory's own policy. Unselected names, aliases and action targets must not
lend labels or counts. Page reads do not update attention, create jobs or write
authority. Native lock discards the generation; unlock does not revive existing
delegations. Expiry, rollback, policy replacement/deletion and instance changes
are checked before projection and before returning its result.

This checkpoint does not yet add a pipe handshake, MCP tools, detail/source/
related/recall adapters or Context trace for the new root. Existing actual-pipe
capabilities remain unchanged. Imported/attachment carrier adoption still
requires the published initialization dependency separately.
