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
