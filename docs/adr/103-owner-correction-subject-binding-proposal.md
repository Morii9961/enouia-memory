# ADR-MEM-103: Native Owner actions and explicit subject binding

Date: 2026-10-27. Status: **Accepted A by Morii; implemented with synthetic validation**.
The human explicitly selected A: bind `owner_subject_id` in the new root.
The alternatives below preserve the decision rationale; B was not selected.
ADR102 is reserved by the concurrent Runtime capture/MCP session.

## Problem and current evidence

G08 and handoff 02 §5 require an actual owner `manual_assertion` plus a
target-bound correction through the System pipeline. ADR93 froze the owner
action/plan/confirmation/record shapes. That record proves the owner's action,
not independent truth or the identity of a statement's subject.

Before this decision, `AutomationDescriptor` bound `created_by: ActorRef` and
`system_actor: ActorRef`; it has no Owner-to-Subject relation. `ActorId` and
`SubjectId` are different types. An automation policy names the target's subject,
not a proof that this subject is the authenticated owner. The API cannot derive
that relation from equal-looking UUIDs, an author label, model text or the target.

Core-1 requires trusted `SourceView.attributed_subject` matching the input
subject for `clear_self_report`. Its documented meaning is “the subject said X”.
`inference` instead yields hypothesis/staged. Native `source_in_policy` also
rejects a known attributed subject that differs from the policy subject.
Current machine capture accepts Runtime events only; parsing a manual assertion
or prospective owner-action record grants no publication capability.

References:

- [Core-1 evidence and attribution](../contracts/CORE1_AUTOMATIC_ADMISSION.md).
- [SourceView and evaluation](../../crates/enouia-memory-contract/src/next/rules.rs).
- [Descriptor and capture](../../crates/enouia-memory-contract/src/next/storage.rs).
- [Native source scope](../../crates/enouia-memory-vault/src/automation/segmented/state.rs).
- [Frozen owner shapes](../../crates/enouia-memory-contract/src/next/owner_actions.rs).

The runtime requirement alone therefore does not determine whether a correction
should be an adopted attributed statement or an explicitly staged interpretation.
Choosing silently would decide an identity/knowledge policy before initialization.

## Concrete alternatives

### A — Bind the owner's subject in the new root (recommended)

Create a new empty `automation-segmented-owner-v1` root. Add an optional
`owner_subject_id: SubjectId` to the descriptor type, mandatory only for this
layout and forbidden on existing layouts. No in-place header conversion.
`created_by` remains the owner principal; this one additional field binds its
statement subject. Root creation requires the trusted native initialization
entry to supply both identities explicitly. Synthetic tests supply synthetic IDs;
implementation must not guess a real identity or open a private root.

Prospective descriptor fragment:

```json
{
  "layout": "automation-segmented-owner-v1",
  "created_by": {"actor_type": "owner", "actor_id": "<native-owner-id>"},
  "owner_subject_id": "<native-owner-subject-id>"
}
```

This fragment is explanatory, not a complete valid descriptor fixture.
Existing fields are omitted here only for readability.

For the first G08 entry, owner correction is allowed only when the selected
current target's subject equals this binding. The newly confirmed manual source
then has that trusted subject; a literal self-report can pass the existing
Core-1 attribution rule. It means the owner said the assertion, never independent
verification. Form-specific Core-5 constraints still apply: subjective/theme
interpretations do not become attributed facts by force.

A foreign-subject correction refuses without source/record publication. Do not
substitute the target subject. Supporting owner statements *about* another
subject would need a separately specified attribution/scope path.

Impact: initialization must know this additional root identity field. ADR93
memory/entity/conversation/owner-action record fields stay unchanged. Existing
roots and currently published MCP layout support remain unchanged; new-layout
adoption is explicit and never inherited from System read authority.

### B — Keep owner assertions unattributed and corrections staged

Still use a new empty owner-capable layout, but add no Owner-to-Subject field.
The manual source records the real owner operator and trusted surface, with
`attributed_subject: null`. The narrow owner entry transcribes the confirmed
text; it never presents it as the target subject's self-report.

System correction uses the current target's existing scope and a conservative
`inference` interpretation, producing hypothesis/staged under Core-1. A real
owner-action record may justify “Corrected by you”; that badge does not change
knowledge/adoption. This applies even to the owner's own preferences because
there is no trusted self-subject binding.

This needs an explicit owner-only source validation path: the generic Runtime
capture rule remains unchanged and cannot accept unattributed manual sources.
Exact target/source and policy boundaries still apply; the new assertion grants
no unrelated source, principal, Context or egress authority.

Impact: no additional root identity field, but the UI must accept that owner
correction does not automatically produce an adopted self-report. Future
attribution support would require another explicit contract/layout decision.

## Common G08 work after the decision

The chosen implementation will add a native Owner factory, current-snapshot
plan/impact preview, private non-deserializable one-use confirmation, witnessed
atomic owner publication and operation-ID recovery lookup. Serialized fields,
Agent/System calls and machine notes cannot manufacture the confirmation.

Keep original source and machine data separate from the owner record. A
correction's System result remains a machine result with its actual rule,
interpretation and receipt. Owner action provenance does not mean that the owner
approved every machine-produced annotation or independently verified a fact.

Withdraw/forget must enforce watermarks and dependent projection invalidation;
the exact target action does not silently authorize deletion of unrelated
original sources or claim physical purge. Attention and family overlays must
use actual owner records, preserve sensitivity/deletion checks and qualify
Jaccard inheritance. Entity merge/split must retain each memory's own source
policy; do not import foreign aliases through an unrelated grant. Refuse
incompatible scopes instead of silently widening them.

Core-5 currently does not permit correction revisions of Theme records: it
requires theme synthesis while revisions reject synthesis as a revision change.
Preserve that rule in the first owner entry; theme rename/pin/dissolution are
owner projection actions. Do not rewrite Core-5 to make every record correctable.

Native synthetic tests will include wrong actor/subject, fake JSON confirmation,
changed snapshot/plan/proposal, expiration/reuse/lock, unknown publication and
exact replay, old-layout rejection, deletion propagation, unauthorized alias
merging and no machine-generated owner badge. No real provider, private root,
Runtime edit/pin, MCP writer or deployment is part of this work.

## Implemented decision

A is implemented as `automation-segmented-owner-v1`, created empty with explicit
native Owner and `owner_subject_id`. Existing roots refuse the added binding;
there is no in-place upgrade. ADR93 record fields stay unchanged. The Owner delta
adds replayable `OwnerActionEffects` next to the frozen action record: scope hash,
caller ceiling, recorded sensitivity floor, full native family membership and an
optional exact literal correction policy/source/decision. No serialized effects
or prospective record confer publication authority.

`NativeMemoryOwner` wraps an explicitly authenticated native principal. Its
plan/gesture-confirm/commit entry uses a five-minute snapshot-bound plan, exact
plan/effects hashes and a private non-Clone/non-deserializable one-use capability.
Lock discards pending capabilities. Commit repeats current authorization, native
clock, snapshot, trusted surface and expiry checks, then publishes one witnessed
Owner transaction. System handles the correction's admission decision under a
new narrow reserved policy; its logical actor remains System. A plan/confirmation
writes no root data. Unknown outcomes require existing forward recovery and
lookup by the original operation ID, never automatic resubmission.

Correction is literal transcription only. It uses a new `manual_assertion` /
`user_statement` source, the actual trusted native confirmation surface/time,
explicit bound subject, original target scope and Core-1/Core-5 rules. It does not
invoke a model or independently verify the assertion. Correction policies cannot
be reused by the generic System admission path or edited as ordinary grants.
Foreign-subject and Theme correction revisions refuse.

Withdraw/forget logically suppress the current target, advance deletion barriers
and hide dependent projections. They retain original sources and immutable
history; no physical purge is claimed. Exact same subject/project/content from
the same evidence SourceId is suppressed even under a new memory ID after reopen;
semantic paraphrase or a newly captured source is not a claimed deduplication rule.
Protect/pin and tags are distinct Owner overlays. Theme rename/pin/dissolve use
full authorized membership, bounded display previews and Jaccard >= 0.5; no
hidden family or deleted original theme can lend its name/provenance. Entity
merge/split require compatible existing authorized entities and every affected
memory's own policy; selected quoted evidence scopes split. Projected aliases,
participants, facets and same-entity relations carry actual Owner provenance.

Read selection version 4 (`owner-qualified-catalog-4`) binds the changed projection
semantics and invalidates old cursors. Process headers use per-port algorithm 2.
Current read receipts omit global commit sequence; conversation metadata omits
raw captured-turn arrays/totals/branches and ranks authorized captured turns
densely per branch. Stored receipts and conversation records remain unchanged.
Old v3 DTO fixtures remain parseable, but native v4 output must not include those
hidden global ordinals. All counts/proofs/edges/marks require current authorization.

Bounds include 64 pending plans, five-minute validity, existing 65,536 commit and
record limits, 1,024 policies (including correction policies), 128 preview targets,
64 displayed unique tags with exact qualified count/completeness, and 16,777,216
identity-action/memory or lazy theme-comparison work. Capacity failures are explicit;
no silent partial exact count is substituted. Source-only original material may
remain readable after target deletion because deletion does not authorize source
purge. Runtime must wire native gestures; MCP receives no Owner factory or token.
See [contract](../contracts/UI_V2_OWNER_ACTIONS.md) and
[synthetic evidence](../validation/UI-v2-owner-actions.md).


The Owner layout's genesis delta includes `descriptor_hash = digest(descriptor)`,
covered by its immutable checkpoint/witness. Replay requires this exact binding
for Owner roots and forbids it for old segmented roots. Editing only an old root's
header cannot grant Owner capability; editing an initialized Owner subject also
refuses. This is native layout metadata, not a change to ADR93 memory/entity/
conversation/OwnerActionRecord fields. The stored Owner delta schema includes both
the frozen record and execution effects; native-emitted genesis/action segments
are included in the independent corpus.
