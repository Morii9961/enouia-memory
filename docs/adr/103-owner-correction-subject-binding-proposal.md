# ADR-MEM-103: Owner correction subject binding (proposal)

Date: 2026-10-27. Status: **Proposed; owner decision required**.
This document grants no authority and implements no owner-action writer.
ADR102 is reserved by the concurrent Runtime capture/MCP session.

## Problem and current evidence

G08 and handoff 02 §5 require an actual owner `manual_assertion` plus a
target-bound correction through the System pipeline. ADR93 froze the owner
action/plan/confirmation/record shapes. That record proves the owner's action,
not independent truth or the identity of a statement's subject.

`AutomationDescriptor` currently binds `created_by: ActorRef` and
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

This fragment is explanatory, not a valid fixture or an implemented descriptor.
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

## Decision requested

Choose A or B before changing the owner-capable descriptor or implementing its
correction publication. A is recommended because it records the identity relation
explicitly and preserves attributed self-report semantics. No code/schema/root
change has been made for either choice in this checkpoint.
