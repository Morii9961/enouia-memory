# UI v2 native Owner actions (ADR-MEM-103)

Status: A explicitly accepted by Morii on 2026-10-27. Native Rust backend only;
Runtime wiring and pin adoption are separate. No Owner route is exported by MCP.
All examples and evidence in this slice are synthetic.

## Root and authority

Create a **new empty** `automation-segmented-owner-v1` root. Descriptor schema 2
adds `owner_subject_id: SubjectId`, mandatory only for this layout, forbidden on
older layouts and never inferred. `created_by` is the actual native Owner principal;
`system_actor` remains a separate logical machine principal. No root upgrade or
conversion is provided. Core-5/ADR94 immutable segmented storage, witness, authority,
forward recovery and deletion/policy barriers remain in force.

The embedding Host creates `NativeMemoryOwner::from_authenticated_owner` only
from its native authentication result. A wire ActorRef, model text, checkbox or
parsed OwnerActionRecord is data, never authentication. System and Agent factories
cannot manufacture this entry. Runtime must keep this object/token outside its
webview/model/MCP payloads and call `confirm_from_owner_gesture` only after showing
the exact native preview and receiving the real owner's confirmation.

## Plan, confirm, commit and unknown outcome

1. `plan(action, max_sensitivity, correction_metadata)` resolves fresh authorized
   current targets, actual native impact, source IDs, five-minute expiry and scope
   hash. It returns a plan, effects hash, first 128 affected targets, **full exact
   authorized affected count**, recorded floor, proposed literal correction and
   explicit retained-source/no-physical-purge flags. It writes no root data.
2. `confirm_from_owner_gesture(operation_id, plan_hash, effects_hash, surface)`
   repeats native snapshot/clock/authorization checks. It returns a private,
   one-use, non-Clone, non-deserializable `NativeOwnerConsent`, stored only in
   native memory. Reconfirmation, changed hashes/snapshot, wrong principal,
   expired scope, lock and serialized fake confirmation fail.
3. `commit(consent)` repeats the current barrier and consumes the pending plan.
   One immutable Owner delta atomically stores the frozen OwnerActionRecord and
   replayable OwnerActionEffects, including a correction's source/policy/machine
   decision and real receipt. Actual committed ID/sequence come from publication,
   never the page. `NativeOwnerOutcome` omits global sequence and assertion text.
4. If publication returns unknown, preserve the original operation ID; use the
   existing native forward-recovery entry, reopen, and query `outcome(operation_id,
   max_sensitivity)`. Do not resubmit a fresh action. A poisoned/unrecovered root
   refuses lookup; a recovered action returns the original witnessed outcome.

Lock discards pending plans/confirmations. Unlock authenticates the same principal.
A new plan is required after restart, expiration or any snapshot change. There are
at most 64 pending plans; capacity errors do not silently evict live consent.

## Stored fields and frontend requirement mapping

| Field/data | Rule | Requirement |
|---|---|---|
| Descriptor `owner_subject_id` | Explicit trusted Owner-to-Subject binding, new root only | G08 correction attribution; Morii choice A |
| `OwnerActionPlan` | Frozen schema-2 operation/snapshot/Owner/time/expiry/action | 01 G08; 02 §5 |
| `OwnerActionConfirmation` | Same operation, exact plan hash, actual Owner, trusted surface/time | 02 §5 |
| `OwnerActionRecord` | Frozen plan/confirmation plus actual commit ID/sequence | 02 §5; machine cannot publish it |
| `OwnerActionEffects` | Scope hash, caller ceiling, stored floor, full family targets, optional correction | Native replay/impact and G18 projection invalidation |
| Correction source | `manual_assertion`, `user_statement`, exact text, real operator/surface/method/time, bound subject | 01 G08; 02 §5 |
| Correction machine record | Target-bound next revision, literal content, actual System rule/interpretation/receipt; `owner-native-literal-1`, live, no model/import | Core-1/3/5; 02 §4–§6 |
| Owner proof/marks | Actual action operation/commit/Owner/time; current corrected/attention/tags | G08 badges; separate from 02 §6 machine annotations |
| Entity/participant overlays | Recorded ID plus qualified canonical ID and actual Owner relation | G09, G14, G08 merge/split |
| Theme overlays | Current qualified family, real rename/pin/dissolve proof, Jaccard >= 0.5 | G05; 02 §7 |

ADR93 title, precision/bound/tz/basis, entities, language, evidence and formation
fields remain unchanged. The new source and decision are validated with those
same fields; suggested tags/relations in machine annotations never create Owner
marks. A correction's prompt version describes literal native construction,
not a model invocation or approval of machine annotations.

## Action behavior

- **Correct:** only a current target whose subject is the bound Owner subject;
  no guessed attribution. The owner supplies exact assertion text and confirms
  proposed title/language/time/entities/sensitivity/interpretation/dimensions.
  Native admission creates the next target revision under a reserved policy
  confined to that target, new assertion source and selected already-authorized
  entity evidence. Generic System admission cannot reuse that policy. Source
  confidence means the Owner said X, not independent truth. Theme correction
  revisions refuse under existing Core-5; no rule widening is included.
- **Withdraw / Forget:** logical target deletion advances the existing barriers;
  current dependent views/history/edges/families stop exposing it. Forget's impact
  hash is computed natively; a page-supplied hash is not trusted. Original sources
  and immutable commit history remain. Exact same subject/project/content from
  the same evidence SourceId cannot be regenerated under a new memory ID, including
  after reopen. No paraphrase detection or source-equivalence claim is made.
- **Protect / Pin:** actual Owner attention overlay; does not rewrite the machine
  attention profile. Optional expiry uses the native read/commit clock.
- **Tag:** bounded explicit targets; unique current visible tags are displayed up
  to 64 with qualified total/completeness, separate from suggested machine tags.
- **Theme rename / pin / dissolve:** native full family membership is stored next
  to the frozen bounded action preview. Recompute inherits only with Jaccard >=
  0.5, preferring the original stable family, then greatest overlap/stable ID.
  A deleted/unauthorized original theme lends no label or proof. Dissolution
  removes the projected family, retaining the recorded synthesis and originals.
- **Entity merge / split:** both entities must already exist, be authorized and
  have compatible access/type/reality/project. Each affected memory must authorize
  the destination from its own policy; another grant cannot lend aliases.
  Split uses current exact quoted evidence and redirects matching mentions.
  Literal/alias search, entity facets, theme/graph relations, neighbors and
  conversation participants derive from the same qualified decisions. Mixed-turn
  splits preserve the original participant and add the selected destination.

## Reads, limits and adoption

Native readers emit selection `owner-qualified-catalog-4` / version 4; restart old
cursors. Process headers use version 2 and each existing per-port algorithm's `-2`
name. Current pages carry Owner marks/proofs and omit global receipt sequence.
Conversation metadata omits raw turns, captured totals and branch arrays; returned
authorized captured turns are densely ranked per branch. Stored receipt/turn
records and old v3 fixtures remain unchanged. Parsing legacy DTOs is not permission
to emit their hidden ordinals in new native output.

Authorize before every count, mark, label, alias, event, source, history row and
edge. Deletion, policy expiry/change, principal/sensitivity/snapshot mismatch
invalidate selection/cursors; unavailable results do not reveal why. Changes from
Owner deltas force projection reload. Outcomes queried by an exact Owner operation
ID expose no assertion body; an unavailable current result is omitted.

Existing 65,536 record/commit, 1 GiB chain and 4 MiB delta bounds apply. The 1,024
policy bound includes correction policies. Preview rows are at most 128 with full
exact count. Entity-action/current-memory work and lazy family comparisons are
bounded at 16,777,216; capacity failures are explicit. Existing read row/byte and
5,000-node graph bounds continue to apply. No persistent projection cache,
embedding, real provider, physical purge or source deletion is introduced.

Implementation: contract `next/owner_execution.rs`; vault `segmented/owner.rs` and
shared segmented state/catalog/process; host `automation/owner.rs` and qualified
catalog/projection/process readers. Schemas and synthetic native examples are in
`contracts/next` and `tests/fixtures/owner`. See [validation](../validation/UI-v2-owner-actions.md).


The Owner layout's genesis delta includes `descriptor_hash = digest(descriptor)`,
covered by its immutable checkpoint/witness. Replay requires this exact binding
for Owner roots and forbids it for old segmented roots. Editing only an old root's
header cannot grant Owner capability; editing an initialized Owner subject also
refuses. This is native layout metadata, not a change to ADR93 memory/entity/
conversation/OwnerActionRecord fields. The stored Owner delta schema includes both
the frozen record and execution effects; native-emitted genesis/action segments
are included in the independent corpus.
