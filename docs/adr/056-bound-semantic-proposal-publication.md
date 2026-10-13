# ADR-MEM-56 — Bound semantic proposal publication

Date: 2026-10-13. Scope: MV-7-H2g, optional native publication and recovery.

H2f drafts require a host to publish an exact owner choice correctly. Add
history_semantic_proposal::publish, taking the saved task, complete OwnerSelection,
inspected head and stable operation key. Validate the completed response, source
revisions, permissions and owner/lock before publication or receipt recovery.
Never send, batch-select or approve a canonical memory.

Bind a published operation to canonical JSON of version, task session/input/hash
and all choice fields (dispatch/response hash/index/content/action/target/revision/
business time). Exclude head, generated IDs and commit times. The operation key is
global within this feature for the authenticated owner; changing content, action
or task under a committed key yields IdempotencyConflict. New publication requires
the inspected head. A matching receipt recovers the original ID after unrelated
head changes, owner edits/review, reopening or target supersession, without
recreating a pending candidate or overwriting the current candidate.

Add low-level govern::propose_bound_at_head and replay_bound_proposal using the
existing CandidatePropose receipt format in a separate namespace. The native
caller computes the exact request hash and enforces permission/deletion checks;
these low-level calls are not authenticated transport endpoints or capabilities.
Legacy propose/edit/withdraw receipt behavior remains compatible. For a concurrent
equal request replayed inside commit, return the winning receipt's candidate ID,
never an independently allocated losing ID.

DuplicateOf stays a non-write result: no durable receipt, no key consumption and
no new model provenance appended to the existing candidate. A host must display
this distinction and must not report a duplicate as a newly persisted analysis.
The host also serializes publication with lock/close lifecycle operations; the
enabled check alone does not provide that serialization.

Saved-response recovery rechecks current sources and the current candidate. Source
revision/deletion or denied access blocks delivery even for an existing receipt.
Only trusted owner plan/confirm can accept or supersede canonical memories.
No new wire/storage schema, dependency or command. Runtime product UI stays in
Runtime. Fault fixtures do not prove power-loss durability or live model quality.
