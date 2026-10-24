# ADR-MEM-83: Workspace explicit supersession draft preview

2026-10-23. Adopted for the local Workspace contract; Runtime adoption is deferred.

Expose the existing ADR-MEM-51 supersession_draft helper as the read-only
supersession_preview command. The owner explicitly selects candidateId,
candidateRevision, targetMemoryId, targetRevision and effectiveFrom. All five
fields are required. effectiveFrom is an exact business timestamp or the literal
unknown; source occurrence/capture time never supplies an implicit effective date.
The request uses a null idempotency key and rejects extra fields.

The helper remains authoritative for current revisions, pending create candidates,
active intact visible targets, exact topic identity, changed content, source
freshness and original provenance. Current candidate edits are preserved.
No matching algorithm, latest-wins rule or domain mutation is added.

The result has readOnly, base, selection, target, proposal and origin. The closed
mapping exposes the target ID/revision/type/content/status, draft proposal fields
and evidence locators, and original origin/actor/extractionRunId/confidence.
It contains no native paths or credentials. A draft is not a candidate, plan,
confirmation token or authority to approve. Nothing is persisted. Existing
propose and review gates are still required for a future authorized write.

Serialization is covered by the unchanged-head and enabled checks; a changed
snapshot, lock or result over 512 KiB refuses the read. Host/MCP/cloud activation,
Provider activity, Runtime pin changes and product UI are outside this slice.
A future Runtime consumer must bind selection/base to its current observation,
invalidate on lifecycle/selection changes and render unknown time explicitly.

Evidence: [Workspace supersession preview v1](../validation/Workspace-supersession-preview-v1.md).
