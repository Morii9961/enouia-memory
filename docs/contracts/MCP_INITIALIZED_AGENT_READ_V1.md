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
