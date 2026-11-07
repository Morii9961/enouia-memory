# ADR-MEM-124 — Independent Agent supersede reports

2026-11-04. Authorized MCP-4 route after ADR123.

## Decision

`memory_supersede_report` reports a fact that was true but has now changed.
It is distinct from an erroneous statement's correction and from forgetting.
Native Owner setup grants an independent AgentSupersedePermit; note, correction,
read and MemoryPropose grants cannot supply it. Use the initialized Agent gate
on the same leased Owner Vault and current policy/source/target closure as
ADR120/121. The exact captured Runtime User/UserStatement quote is evidence;
the Agent's bounded `supersede` interpretation is untrusted, never content.

Use independent capability memory-agent-supersede-native-1 and request/key
space. Persist a closed agent_supersede delta on the existing witnessed Owner
chain. The receipt state is supersede_report_saved, with original target,
report key, request ID/hash and commit only. Saving changes no memory or recall,
creates no Owner action and holds no human review queue. New work requires the
exact current target; an authorized original-key receipt may replay after a
System revision. Revocation/deletion/policy/scope checks precede receipt lookup.
Capacity and unknown-publication recovery follow ADR121, independently.

The leased captured worker gains a separate SupersedeAnalyzer port. It selects
saved supersede reports in key order, journals supersede_key before dispatch,
and analyzes each at most once with the current target. A single resolvable
user statement with change=update may create an ordinary target-bound revision
through the existing evaluator/apply_batch; correction or any other change is
refused. Bind the dispatch target, preserve its history and sensitivity, and
refuse a moved target rather than retargeting it. Deleted inputs/inactive policy
close without analysis. Paused proves no handoff; Unknown/panic/timeout/restart
is terminal without resend. Capture, correction and supersede dispatch do not
overlap. Host opt-in start_worker_with_reports drives the three explicit ports;
existing start_worker/start_worker_with_corrections retain their behavior.

Expose five initialized reads plus memory_supersede_report through distinct
memory-agent-pipe-8, profile memory-agent-initialized-supersedes-1 and wire
capability memory-agent-supersede-1. Pair independent native read and supersede
delegations for the same Agent/exact leased Vault and OS enrollment. Exact
closed hello binds the descriptor, Vault, read fingerprint and reachable wire
schemas. Existing pipe-5/6/7 meanings do not expand. Explicit launcher attach
mode is automation-supersedes-v1. Unknown publication emits no tool result or
Reply, retains the complete original MCP Value, revokes both grants and actually
disconnects. No automatic resend/reconnect/new key; explicit original-key
recovery only. Stdio keeps the full Value, including on output failure.

Closed schema-2 delta and optional supersede_key on the same journal v1 path;
older binaries must not open roots containing new deltas. Read fingerprints
including catalog-v2 change; use matching revisions. No real model/Provider,
egress, Runtime pin/configuration, Owner method, forget/hide or MCP-5 is added.
See [contract](../contracts/AGENT_SUPERSEDE_V1.md).
