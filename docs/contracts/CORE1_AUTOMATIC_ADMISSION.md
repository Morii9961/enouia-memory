# Core-1 automatic admission contract

Date: 2026-10-24. Frozen by [ADR-MEM-85](../adr/085-machine-admission-drafts.md).
Implementation baseline: `9286cd8f89669d9ec3b92f943fd74072457536e7`.
Scope: isolated synthetic contracts and pure rules, with no persistence adapter.

## Types and trust boundary

`enouia_memory_contract::next` exports four closed, independently tagged
documents in [automation-v2](../../contracts/next/automation-v2.schema.json):

| Document | Meaning |
| --- | --- |
| `AutomationPolicy` / `automation_policy` | Exact System principal, Vault, subject/project, source IDs, create permission/target IDs, sensitivity ceiling, output access/egress IDs, validity, revision and rule version |
| `AdmissionInput` / `admission_input` | Exact snapshot, logical operation, actor, policy revision/hash, producer/model versions, statement interpretation, selected source revisions, evidence/context, optional target revision/hash and prospective result |
| `DecisionDraft` / `admission_draft` | Full immutable input plus deterministic knowledge/adoption/reassessment outcome and rule version |
| `PlannedCommit` / `planned_commit` | Prospective commit ID, exact base/primary/policy/deletion binding, operation, decision digest and result revision; no success or receipt field |

Document `schema_version` is exactly **2** using a separate `NextVersion` type.
The existing global `SCHEMA_VERSION`, `SchemaVersion`, RecordKind, AnyRecord,
RecordSet, writer operation kinds and schema-1 files are unchanged. None of
these four types implements `Record` or a store write port. Policy revision is
independent of schema major; the only supported rule algorithm is
`admission-rules-1`. Producer/model versions describe analysis provenance and
grant no authority. `model_version: null` is used by the synthetic baseline.

`parse_document` bounds input to 256 KiB, rejects unsupported majors, duplicate
decoded keys, unknown fields, unsafe integers and local violations. It parses
original bytes after version preflight. Structural validity does not authorize
evaluation or publication. Error rule/path output contains no source body.
Arrays are bounded at 128; content at 16 KiB UTF-8; producer/model versions at
128 UTF-8 bytes. Schema character limits are an outer shape check; native byte
limits, real dates and cross-document rules can be stricter.

`EvaluationContext` is an in-memory native trust boundary, not request JSON.
It supplies the actual System actor, exact active policy, current snapshot,
caller-supplied evaluation time, current source views and current target views.
The API does not authenticate that context. A future adapter must obtain it
from the single legal Host and current authorization/deletion resolution, never
from page, model, MCP arguments or source instructions. Caller-supplied policy
bytes and `actor_type: system` alone do not create System authority.

The policy and input actor must equal the actual System identity. Owner, Agent,
Provider, Client and Device cannot substitute. Scope is exact, including null
project, and expiration is exclusive. Policy hashes cover the complete canonical
document; changing any policy field/revision requires a fresh binding. Output
access and egress IDs are fixed by policy; egress ID records a restriction, not
authorization to send. This slice performs no network or policy persistence.

## Evidence, dependencies and pure transition

All supporting and context refs must exactly account for `input_refs`. Missing
context, undeclared support and unused selected refs refuse the draft. Each
source must resolve uniquely at its current exact revision, pass existing v1
SourceRecord validation, be complete/live/readable, be policy-selected and have
the exact output access policy. Mixed access policies are conservatively refused
rather than invented intersections. Sensitivity inherits the maximum of every
support/context source and the target; no declassification path exists here.

Evidence must name the actual source class, exact anchor hash and a locator
within the stored locator. `supports` is exactly `content`. Supporting entries
cannot repeat a source identity, exact source content hash or upstream message
identity (provider/account/conversation/message). Same text in independently
recorded events is conservatively refused as duplicate support in Core-1.
This is not a complete semantic lineage/independence detector. Rewritten model
summaries cannot earn corroboration: no corroborated/verified output or evidence
count threshold exists. Context sources never count as supporting evidence.

| Supplied interpretation | Knowledge | Adoption | Reassessment |
| --- | --- | --- | --- |
| clear_self_report, with direct user evidence and trusted subject attribution for every support | attributed_statement | adopted | none |
| inference | hypothesis | staged | on_evidence_or_policy_change |
| ambiguous | uncertain | staged | on_evidence_or_policy_change |
| conflicting | disputed | staged | on_evidence_or_policy_change |

Attributed means **the subject said X**, including a preference or self-report;
it does not establish X as independently true. Subject attribution is separately
resolved by the trusted adapter; speaker `user` or an author label alone is
insufficient. `SourceView.attributed_subject` must match the claimed subject.
Every interpretation requires actual source support. Missing/forbidden/stale
dependencies produce a refusal, not a human-review task.

Staged is system-maintained non-adoption: new evidence or policy can trigger a
fresh evaluation, or the system can abstain. It creates no mandatory review
queue. Core-1 records the trigger semantics but has no scheduler or worker.
Knowledge/adoption are separate fields; neither is attention, currentness or
retrieval rank. A future read model must preserve the attribution wrapper when
using an adopted self-report in Context. The rules consume semantic labels;
they do not prove quotation entailment, identity resolution, negation handling,
conflict discovery, completeness or model quality.

New results require revision 1 and a currently absent memory identity. Revisions
require an exact current V2 target revision/hash, scope, write permission and
result identity at revision + 1. Legacy owner records, deleted/unwritable targets,
stale hashes/revisions and foreign subject/project/policy are refused. Staging
a revision is only a prospective result; it does not supersede the adopted
target or imply that a stored current view has changed.

`validate_commit_binding` reevaluates the decision against the supplied current
context and checks exact snapshot, operation, decision digest and result. It
returns `Result<(), ContractError>`, never a receipt. It also rejects a planned
commit equal to its base. The commit ID is prospective; uniqueness, atomic
decision/result storage, operation dedupe and success receipts belong to Core-3.
No pure check protects against a race after evaluation. A future transaction
must recheck policy/deletion/primary barriers and all refs before publication.

## Version and consumer matrix

These are separate axes, not a global upgrade:

| Axis | Existing | Core-1 | Later activation requirement |
| --- | --- | --- | --- |
| Source revision | fixed Git pin | new branch only | explicit Runtime adoption |
| Record schema | 1 | unchanged | isolated new-root reader/writer with schema 2 |
| Storage layout | existing segmented layout **2** | unchanged | do not confuse layout 2 with automation schema 2 |
| Host publication | host-current-v2, schema-1 records | unchanged | separate native transaction integration |
| Automation documents | absent | schema 2 drafts/policy only | legal decision/result record set and receipt |
| Workspace IPC | schemaVersion 1, 55 commands here | unchanged | new versioned read capability, never relabel old fields |
| Memory IPC / MCP | existing six-tool meaning | unchanged | separately authorized future adoption |

| Reader/writer or consumer | V1 records/owner history | Core-1 V2 documents | Future V2 storage root |
| --- | --- | --- | --- |
| Existing embedded Runtime at its old pin | matching V1 roots only | refuse typed business decoding/writing | refuse root admission before record decode/query |
| Existing Memory IPC/MCP/Host | existing V1 rules and scopes | no dispatch, no RecordKind or write path | refuse before dispatch/queries; no owner bridge |
| This build's legacy reader/writer | unchanged V1 parser and OWNER_ONLY restrictions | unknown major diagnostic bytes only; no rewrite | not supported in this slice |
| Core-1 `next` parser/evaluator | consumes validated current V1 source metadata; does not rewrite owner history | parse, validate, evaluate, bind drafts only | no reader/writer implementation |
| Future explicitly adopted V2 reader | preserve legacy source/owner bytes and approval provenance | separately versioned machine meaning | must validate complete manifest and legal authority first |
| Future V2 writer | no implicit bulk owner conversion | only legal System transactions after Core-3 | sole writer; no v1 fallback or dual write |

No V2 business Vault reader or writer is delivered by this slice. The V2 parser
is explicitly a document parser, not `parse_any` or support for a new RecordSet.
Unknown-major raw-byte diagnostics never authorize queries, indexing, Context
use, policy decisions, rewrite or export as V1.

For a future new-format root, reserve schema major 2 on **both `vault.json` and
CURRENT**, before publishing any automation records. Its complete storage
schema/transaction adapter still needs the Core-3 implementation. Existing
strict schema-1 descriptor/CURRENT parsing refuses these headers before record
decoding. Never put a new kind inside a schema-1 descriptor/catalog and depend
on old code ignoring it. Do not reuse `format_version: 2` as a gate: it already
means segmented V1 storage. Old clients must only be handed supported V1 roots;
new root selectors/Host handshakes must reject incompatible schema/capabilities
before opening a business query. No query followed by UI field hiding, partial
record skipping, downgrade, empty-success or embedded fallback is allowed.

Runtime at the handoff's `56bc5eb` / 47-command pin stays on its matching V1
root. That pin was read from the supplied handoff, not reverified in Runtime in
this slice. Today's Memory branch having 55 commands does not upgrade it. New
types do not change the old native handshake hashes or existing wire schemas.
Future automation-root native sessions must advertise the new exact schema
capability/hash; existing MCP grants cannot become machine-admission scopes.

## Migration and rollback design (not executed)

1. Quiesce the single source writer and capture the exact commit, complete
   manifests/raw bytes, owner approvals/reviews, authority epoch and policy/
   deletion watermarks. Verify an independent restore and deletion closure.
   Never use a historical snapshot that resurrects deleted data.
2. Build an explicitly selected new-format **synthetic** root first. Retain
   legacy records, review/approval history and provenance as original immutable
   byte objects with an index/mapping, not deserialize/reserialize copies.
   Legacy absence of context is unknown, never a reason to fabricate evidence.
   New machine results use their own namespace/semantics; no fabricated owner,
   ReviewRecord, confirmation or bulk candidate promotion.
3. Validate complete migrated closure and rejection by all old consumers at the
   descriptor/session gate. Adopt matched Runtime/native reader capabilities
   separately and verify Memory, Context, Sessions, Provider and quick-search
   consumers before any real switch. No new root is handed to the old pin.
4. A later explicit activation fences the old writer and selects exactly one
   supported primary. Recheck exact source snapshot and all monotonic barriers.
   Read indexes are rebuildable, never alternate authority. No production
   sidecar or dual writing is authorized by this design.
5. Before new writes, abort can discard the synthetic target and retain the
   unchanged original root. After V2 writes, binary downgrade is not rollback:
   stop writers and require a separately verified forward recovery or explicit
   compatible conversion. Never lower policy/deletion/primary watermarks or
   open an old backup as a second primary. Disabling automation leaves machine
   provenance intact; it does not turn it into an owner-approved record.

Real migration, restore/cutover tooling and a concrete new storage set validator
are later work, not a hidden Core-1 dependency or a claim of migration safety.

## Future jobs and minimum Runtime read mapping

Core-2 can bind a persistent job to snapshot/operation, exact input and policy,
producer versions, decision digest and intended result. It still needs separate
job identity/state, input watermarks, cursor, dedupe, cancellation, lock/root/
epoch invalidation, unknown outcomes, budgets and resource isolation. Model and
embedding work must occur outside the writer exclusion interval. Staged entries
need automatic reevaluation triggers and bounded retention/abstention, not an
owner obligation. No job runtime or continuous Provider policy is implemented.

The future minimum read capability must expose authorized bounded list/detail,
machine vs legacy-owner formation, exact ID/revision/snapshot, knowledge,
adoption, evidence/context separately, policy/rule provenance and completeness.
Machine adopted maps to “automatically adopted; subject said X”, staged to its
knowledge state and automatic reevaluation, legacy approved to original owner
history. Never map machine adoption to “owner confirmed”. A draft or binding
success maps to “not committed”; only a witnessed transaction receipt can say
“saved”. Job progress and attention/currentness remain separate unavailable
capabilities until implemented. Permission filtering must precede counts/edges.

Context consumers must preserve qualified knowledge and abstention; Sessions
retain provisional checkpoint semantics; Provider uses its independent egress
policy; quick search only receives authorized returned results. Runtime owns
UI mounts, shared shell, native lifecycle and credentials. This slice adds no
projection endpoint, full timeline/star map, vectors, notification, MCP tool,
Provider call, Activity/Usage input, worker or Runtime upgrade.
