# Core-6 contextual recall and native maintenance

Date: 2026-10-24. [ADR-MEM-90](../adr/090-contextual-recall-maintenance.md).
Base: Core-5 `e59617012c626d93c6932c2ff5c9355c988e4564`.

## Native selection

`AutomationHost::select_context_native` is an opt-in Memory-native API for
explicit HostV2 synthetic roots. It obtains latest revisions through the live
System/root and current policy, source revision/deletion, lineage, access and
sensitivity checks. Only that authorized set reaches ranking or trace/counts.
Public sensitivity queries cannot observe private candidate IDs, titles or
excluded counts. No MCP/IPC/Workspace/Runtime endpoint or broader external
Context access is added; the existing `context` path remains compatible.

`RecallQuery` is native, not request-deserializable authority. It supplies text,
explicit condition cues, optional event window, Current/Historical/Browse intent,
ranking clock and bounded item/rendered-byte limits. Current validity uses the
fresh native clock, separate from the ranking clock, so old ranking time cannot
revive expired current facts. Historical selection may inspect a past valid
statement but always uses present authorization/source/deletion gates. It does
not reconstruct a counterfactual past knowledge set or restore old grants.

The closed schema-2 `AttentionProfile` is retrieval tuning, not a permission
grant. It fixes `attention-rank-1`, fade/archive horizons, protected forms/IDs,
up to 16 bounded alias groups, result limits and maintenance interval. Profiles
never widen the underlying scope, declare truth, approve memory or authorize
external calls. Unknown algorithm/version and unbounded tuning refuse.

| Step | Rule |
| --- | --- |
| Native authorization | Before candidate construction, counts or exclusions; latest identity only |
| Knowledge/adoption | Staged hypotheses/uncertainty/conflict never enter ordinary statement context |
| Conditions | Every recorded condition needs an explicit matching cue; no implicit universal preference |
| Event/valid time | Unknown event time cannot match an invented date; Current validity uses present native time |
| Candidate relevance | Lexical terms plus explicit profile aliases; no vectors, random old memories or latent model scores |
| Attention | Active/fading/archived/protected is separate from truth, adoption and plan state |
| Ranking | Lexical count x100, alias match +60, active/protected +3, fading +1; stable identity/revision ties |
| Budget | Exact qualified rendered UTF-8 bytes including separators, plus item cap; overflow/truncation explained |

`RecallTrace` includes algorithm, snapshot, profile hash, ranking/authorization
times, authorized considered count, selected exact revisions and reasons, plus
authorized exclusion reasons. Reasons distinguish lexical/alias/condition/time,
protection, freshness/fading, historical rescue and budget. There is no hidden
record count or inaccessible edge list. Scores describe this algorithm only;
they are not confidence, evidence strength or cognitive-science measurements.

## Fading and original evidence time

Attention age uses the newest captured time among original selected sources.
It does not use search frequency or a fresh summary's creation time. Repeated
reflection over the same evidence cannot refresh that age. Source capture time
is an attention basis, not event time or the commit's learned time.

Fading/archive only alter participation in this native default selector. An
explicit related Historical query rescues eligible archived material; protected
episodes/relationship experiences remain available when relevant. Protection
does not override ACL, deletion, staging, conditions or time. No age rule marks
a plan completed, a pause abandoned or an old event false. Records/history are
unchanged; this is not physical archive movement, logical deletion or purge.

Search/read is pure. Repeated calls do not touch, reinforce, alter approval,
produce feedback or create evidence. The existing minimal reader also preserves
its previous behavior; adopting this selector is explicit native integration.

## Memory-owned maintenance

`AutomationHost::start_maintenance` starts at most one separate private bounded
Memory maintenance thread for the owning Host, in addition to its single
generation worker. No Runtime/general worker pool or frontend timer is used.
The interval is 100–60000 ms for synthetic testing/tuning. Each short tick
rechecks the native root through the Host lifecycle gate, then atomically writes
`indexes/core6-attention.json` only when its bytes differ. There is no model call,
embedding, Activity/Usage lookup, notification channel or action executor.

The closed `AttentionProjection` contains only exact memory refs, attention
state, source time, snapshot, profile hash and bucketed native evaluation time.
It is disposable metadata, not memory or permission authority. CURRENT and
record/receipt/knowledge/state histories do not change. A tick racing a newer
snapshot can leave an older labelled projection; business reads recompute via
fresh authorization and never use cached eligibility. Corrupt/stale cache can
be replaced without repairing memory. Do not expose raw index counts to clients.

Lock pauses ticks and reads. Cancel/Drop prevents future ticks and wakes the
wait; `join` waits for the bounded in-flight metadata tick. The worker uses a
weak Host reference and a running flag, so it cannot keep a closed root alive
indefinitely or permit duplicate maintenance threads. A tick error pauses that
attempt; an index retry creates no memory operation or Provider resend. A panic
is contained in the worker boundary. No API executes a remembered action.

There are no notifications in this slice: quiet hours, delivery/retraction and
offline notification backlog are unavailable rather than implicitly granted.
Deleting sources immediately filters reads even if a projection is old; next
maintenance drops those rows. Physical purge of old private indexes and whole
filesystem rollback/independent restore remain later migration work.

## Versions and remaining evidence

Existing schema-1 records, OWNER_ONLY, rule-1/2/3 storage, Host/MCP/Workspace
semantics and Runtime pin remain unchanged. No new root layout or business
record kind is needed for a rebuildable index. Older Core-5 HostV2 readers can
continue their old read behavior and ignore this disposable file; they cannot
claim the new selection capability. V1/old-layout consumers still refuse the
HostV2 root at their descriptor gate. Unknown profile/projection kinds are not
interpreted as business records or write authority.

See [validation](../validation/CORE6-recall.md) for synthetic baseline comparison,
deterministic replay, permission/deletion, conditions, date/age, byte budgets
and real native periodic-thread checks. Alias fixtures do not establish general
semantic recall quality. Real-model evaluation, vectors, graph expansion,
personal data, production tuning and Runtime UI/native adoption are separate.

MCP-2c adds the explicit `select_catalog_recall` entry point for up to 65,536
already-authorized current candidates, using the same deterministic selection
logic. `select_recall`, attention projections and legacy simulation schemas keep
their 128-candidate limits. Its potentially large internal trace is published
only through the separate [catalog Agent contract](MCP_AGENT_CATALOG_READ_V1.md),
with bounded exclusion identities and exact coverage/reason counts; it must not
be mislabeled as a complete legacy trace.
