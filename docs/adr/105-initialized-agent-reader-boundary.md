# ADR-MEM-105 — Explicit initialized-root Agent adoption

2026-10-28. MCP resumption after the published ADR103 Owner implementation.

Integrate exact dependency 34709b9d2abcb60188c0e3488b15f40c33d6ee16, including
its ADR100/101 prerequisites, without exposing its System/Owner ports through
MCP. Preserve ADR102 raw capture, preflight, unknown outcomes and the existing
Agent gates. Keep both sides' contracts and validation corpora.

automation-segmented-owner-v1 is a new root, bound to explicit owner_subject_id
and immutable genesis descriptor_hash. It is not segmented-host-v1 with extra
privileges. Existing Agent catalog and Runtime native capture factories must
refuse it at their descriptor boundary before source/query/transaction work.
The old factory's layout check precedes snapshot inspection; native capture's
layout check precedes source/policy planning. No client principal/scope/PID or
Owner-shaped request can select a different capability.

This first checkpoint integrates and tests refusal, not the initialized Agent
reader. The later reader must negotiate a distinct explicit initialized-root
capability and independently qualify current policy, source reads, deletion,
withdrawal, Owner overlays, aliases, counts and dense visible ordinals. Reusing
pure projection rules is permitted; forwarding a System reader or giving Agent
access to NativeMemoryOwner/consent is not. Keep the same trace for actual Agent
recall and native Context. Read activity is not attention reinforcement.

Initialization dependency 2c7310a81cd086b17c702d916d1d8ea51a75eb6a and its
expanded import/attachment source carrier require separate exact adoption.
Neither a dependency merge nor compatible schema parsing authorizes use of a
new format. No old root is upgraded, no Owner action is executed by MCP and no
new writer or broader Context is introduced here.

See [boundary contract](../contracts/MCP_INITIALIZED_AGENT_READ_V1.md).

Checkpoint 2 introduces only memory-agent-qualified-native-read-1 on Owner
roots. It is a separate native Agent delegation and scoped page, not an MCP
advertisement. Reuse the pure catalog projector after the Agent gate; never
construct or forward a System reader. Recompute Owner overlays after filtering
memories, sources and entity evidence to the delegation. Do not lend a canonical
entity or alias from an unselected source. Keep raw Owner records off the output.
Bind page selection/cursors to this delegation's scope hash. Lock/unlock cannot
revive old readers. Root/policy/clock/instance/deletion checks remain current.
The native factory assumes trusted native Owner authentication; this constructor
alone is not OS enrollment. Actual MCP process/transport binding follows later.

Checkpoint 3 adds native detail/source/related/recall methods to that same sealed
delegation. Source text needs the independent source-read bit. History uses its
original revisions and safe scoped receipts, not current Owner badges pasted
onto old records. Related results distinguish recorded relations from algorithm
suggestions; conversation relations remain unavailable until their own Agent
qualification exists. Recall accepts attention already qualified by catalog-4,
without changing stored memory or the original selector's behavior. Context
returns the actual same cached response under current scope/snapshot checks.
No new MCP advertisement or initializer-carrier adoption is implied.

Checkpoint 4 freezes the distinct pipe-4 / memory-agent-qualified-read-1
five-tool profile in MCP_OWNER_AGENT_PIPE_V1. Native qualification stays separate
from OS process enrollment and wire input. Bind exact layout/carrier/schema hash
before dispatch, keep old clients refused, and bound history construction with
delegation/snapshot/profile/memory cursors. Initializer import/attachments still
require an exact published dependency and deliberate compatibility update.

Checkpoint 5 adopts the exact published initializer dependency
3b6e974cc12182859d3645d64c03e5acc534896f, preserving both histories. Its wider
source carrier is Core support, not permission for an existing Agent profile.
Original Agent factories/permits check every retained source revision before
projection and counts, including unselected or deleted sources. Existing
witnessed native Owner correction/manual-assertion sources remain supported.
A native import
after enrollment invalidates the old reader; deletion cannot downgrade carrier
compatibility. This check uses the already loaded, sole leased native Vault;
it is not a claim to reject its internal storage decoding before native open.
Pipe compatibility/liveness still rejects before external business dispatch.

The old nonsegmented capture entry also refuses typed attachments, matching
the old segmented entry. Initializer import APIs remain Owner-layout-only.
A later explicit initialized-carrier native/wire profile must authorize typed
attachment metadata and duplicate refs/extensions before exposing Source/Detail.
No new Agent capability, initializer reader, live queue or Runtime hook is
advertised by this integration checkpoint.

Checkpoint 6 adds the explicit memory-agent-initialized-native-read-1 delegation
adopts ADR104 carriers under the existing independent native Agent gate. The
original factory/pipe-4 still refuses them. Source/Detail are authorized metadata
projections; typed attachment floors, duplicate refs/extensions and timezone
origin must be handled before serialization. No new wire advertisement or System
forwarding. See the checkpoint 6 contract and actual validation report.
