# ADR-MEM-104: Synthetic initialization through shared native formation

Date: 2026-10-27. Status: synthetic implementation; real input/egress and Runtime
adoption remain separately authorized. Morii explicitly confirmed ADR-MEM-93 in
the initialization chat, then allowed the minimal shared-source integration.

## Decision

Use ADR-MEM-93 record fields and ADR-MEM-103's new Owner segmented synthetic root
(updated 2026-10-29 from the original ADR-MEM-94 pilot).
Reuse native policy, evaluation, revisions, witnessed application, deletion and
the ADR-MEM-100/101 readers. No existing root is upgraded. The initializer is a
separate crate with explicit in-memory material selection and a synthetic-only
example. It never discovers directories, opens a real legacy Vault, invokes a
Provider, changes Runtime, or selects a timezone from the host environment.

Preserve selected original bytes under the existing content-addressed Raw
layout. Parsing and preflight are pure. Parse warnings and unknown language,
time, author and parent remain explicit. Preserve conversation/message identity,
parent branches, assistant model, source timezone, Runtime event metadata and
bucket annotations. Bucket text is agent material, never owner evidence.

The ordinary runtime capture entry still accepts only complete runtime events
and cannot create UserConfirmation. A separate native `capture_import` accepts
imports and agent submissions from the trusted native parser, verifies object
hash closure, and rechecks legacy owner evidence against the complete selected
v1 RecordSet, exact content hash and Review association. Tombstoned memories and
their supporting/context dependencies are excluded. Original Review bytes are
retained; no new owner action, manual assertion or confirmation receipt is made.
Inherited review metadata without an exact text witness remains a model claim.
Deduplication retains metadata changes: a repeated text with newly supplied
model/timezone/attachments is a preflight reconciliation conflict, not a silent
discard. Explicit selected document metadata is bound into the input watermark.

CapturedSource also retains optional typed v1 AttachmentRecords, cross-bound to
its source ID, attachment refs, sensitivity and verified bytes. Empty lists are
omitted, preserving earlier synthetic fixture hashes. Unknown attachment types
and URLs remain inert. Existing root/record versions remain unchanged; older
strict segmented consumers reject populated attachment fields or imported
source kinds and must not be offered these roots. Existing Runtime pins reject
the segmented descriptor before business use. There is no compatibility shim.

Backfill and live share Plan -> native prepare/evaluate -> witnessed apply.
Scripted analysis supplies independently labelled synthetic statements, explicit
revision targets and synthesis parents. It is not a semantic model or a proof of
title entailment, identity, completeness or extraction quality. One statement
uses exact scalar quote offsets; the System decides adopted/staged under Core-1.
Event/valid time is supplied explicitly and never falls back to source/import
time. Conversation start/end can use explicitly labelled original source time.
Revision-bound tags, tone, importance, state notes and suggested relations remain
advisory. Unsupported titles fall back to a quoted-text prefix. Preflight reports
field coverage and an estimated staging count; native current evaluation decides
the actual outcome at publication.

Only a new empty `automation-segmented-owner-v1` root with explicit
`owner_subject_id` is accepted. Native genesis witnesses `digest(descriptor)`;
the initializer's separate `initialization-owner-1` journal binds that digest
and refuses a changed attributed owner. There is no old-root header upgrade or
subject rebinding. Owner catalog selection uses v4 and process headers use v2.

Explicit native-selected `OwnerProvided` input is an ordinary user-statement
source in normal formation, not a pending-review queue or a G08 correction.
An optional original quote is preserved separately with its original speaker
and evidence class. JSON consent claims cannot choose this parser mode or
create Owner confirmation. The production caller must authenticate source
selection independently; the synthetic parser supplies no authentication.

The initialization journal is bounded, root-bound and exclusively locked. Save
exact batch identity/input watermark and analysis hash before capture. Original
Raw is durable first. Save request/response/binding and application intent before
native application. A crash with an uncertain application performs original
operation receipt lookup only. Missing receipts keep the operation unknown;
there is no blind resend, new identity, automatic rebasing or approval.
Live wins before each bounded step; already committed backfill is retained.

Seven disposable projection groups are rebuilt through currently authorized
native reads. Envelopes use ProjectionMetadata with exact snapshot, scope,
withdrawal/deletion barriers and original dependencies. Search/attention bundle
rows reuse the native authorized list; themes, typed graph, time buckets, facets
and activity reuse their native ports. Saved bundles are inspection artifacts,
never read authority. Deletion invalidates old envelopes; rebuilt groups omit
both deleted results and derivations whose support or context was removed.

## Limits and acceptance

The pilot takes at most 64 parsed sources and 128 scripted statements per plan,
with a 16 MiB material/journal bound. Capacity refuses rather than truncates.
Repeated identical received bytes and exact queued plans dedupe. Different
exports of the same upstream message are not yet a general reconciliation
engine; changed analysis under the same input/mode conflicts. Raw bucket YAML,
binary document extraction/OCR, real legacy snapshot acquisition, semantic
models and a production scheduler are unavailable. These are reported gaps,
not silently inferred metadata or permission to process private files.

G08 execution is supplied by the Owner dependency. One integration test applies
an explicitly simulated native Owner protect gesture after initialization;
ordinary initialization creates no OwnerAction or consent receipt. Already
captured Runtime source revisions still need a separate queue-only API; they
must not be recaptured as imports. Runtime screenshots
and six-view adoption require its separate adopting slice and are unrun.
See [initialization validation](../validation/Memory-init-pipeline.md) for actual
measurements, scenario boundaries, checks and remaining differences.
