# ADR-MEM-85 — Isolated machine admission drafts

Date: 2026-10-24. Status: Adopted (contract and pure rules), Core-1 only.

Morii authorized automatic memory formation without mandatory human review.
This decision adds a separate next-version contract; it does not activate a
writer, migrate a root, or change existing owner approval semantics.
It amends ADR-MEM-05/22 only for the explicitly selected future automation
format: their schema-1 owner-review rules and ADR-MEM-30 approval/egress rules
remain in force on all existing paths.

## Decision

Use `enouia_memory_contract::next` for bounded version-2 automation policy and
admission draft documents. Keep the existing global record schema at 1 and
RecordKind/AnyRecord closed. These documents implement neither Record nor any
Vault write port. V2 here is a document namespace, not host-current-v2 (whose
records still use schema 1), Workspace IPC v2, or a released V2 Vault reader.

Only an actual System actor supplied by a native trusted evaluation context
can evaluate a draft. The input actor must match that actor and the exact
versioned policy. Source speaker roles and claimed consent never grant this
identity. Policy scope binds Vault, subject, optional project, selected sources,
allowed new/target memories, sensitivity ceiling, output access/egress policy,
validity and the fixed rule version. Policies are inputs to pure validation;
this slice does not authenticate, persist or grant them.

Inputs name an exact authority/data snapshot, source revisions, supporting
citations, every other analysis-context dependency, an optional exact V2 target
revision/hash, and the intended result revision. Trusted current source views
must be live, readable and complete. Citations must match actual source class,
object anchor and locator. All selected sources must be accounted for; context
is never extra evidence. Repeated source identities, identical content, or
duplicate upstream message identity cannot become independent support.

The rule engine maps a clear self-report with direct user support to
`attributed_statement` / `adopted`: this means the person said X, not that X was
independently verified. Inference, ambiguity and conflict become respectively
`hypothesis`, `uncertain`, or `disputed`, all `staged` with automatic reevaluation
on new evidence or policy. No required owner queue and no independent-truth or
corroboration output exist in this slice. Semantic classifications are supplied
analysis, not proof that a model understood negation, identity or context.

The decision embeds the full input, policy binding and deterministic outcome.
A separate planned commit binds its digest, exact base/authority watermarks,
operation and intended result. Binding validation returns only unit success;
it is not a durable commit receipt. Future legal transactions must revalidate
all freshness/security barriers, commit result and decision atomically and only
then issue a receipt. No fake ReviewRecord, owner confirmation or approval.

## Compatibility and activation

The [Core-1 contract](../contracts/CORE1_AUTOMATIC_ADMISSION.md) freezes the
reader/writer and consumer matrix, predecode refusal, migration/rollback plan
and future job/read-model mapping. Old record bytes and approvals are immutable
inputs to a future migration, never reserialized or batch-promoted here.
Legacy-owner records are not legal automatic revision targets in this slice.
Unsupported document versions refuse parsing/evaluation; unsupported root
formats must refuse before record decoding or querying. Raw-byte diagnostic
retention is not a business reader or a downgrade path.

## Scope and evidence

No default entry point, CURRENT, OWNER_ONLY set, IPC, Host/MCP behavior, Runtime
pin, Provider call, worker, index, private data or production root changes.
Existing module exports and the new isolated source/schema files are recorded
in the Runtime surface; adoption remains separate. Evidence and limitations:
[Core-1 validation](../validation/CORE1-automatic-admission.md).
