# Workspace explicit supersession draft preview v1

2026-10-23. Stacked on history-brief source 08892702d0d0104b2d23240f89d2130c4dc1d323.

This slice adds a read-only Workspace wrapper and typed/schema/catalog wiring
around the unchanged ADR-MEM-51 supersession_draft helper. Required input is the
explicit candidate and target IDs/revisions plus effectiveFrom; unknown business
time remains unknown. Current candidate edits, explicit target and original
provenance are preserved. There is no publication, review plan, confirmation
capability, canonical write, Provider call or Runtime adoption.

Checks actually run:
- cargo fmt for enouia-memory-contract and enouia-memory-workspace.
- cargo check --offline --locked -p enouia-memory-workspace passed; log:
  target/supersession-preview/check.txt. This is compilation, not runtime behavior.
- The integration surface manifest was regenerated and its aggregate recorded
  in docs/integration/RUNTIME.md; the required consistency check passed.
- git diff --check passed. The staged scope is limited to this wrapper,
  contract/schema/catalog fixtures and its ADR/integration/evidence documents.

Existing helper behavior evidence remains in
[MV-7 history review](MV-7-history-review.md) and ADR-MEM-51. That evidence is
reused for the unchanged helper; it is not a newly executed Workspace test.
No new test file or suite was added. The catalog gains one request/response
example; independent schema execution and wrapper behavior tests were not run.

Still unverified: executing this new wrapper against synthetic or real records,
actual Runtime consumption, physical UI, owner acceptance and personal-history
quality/coverage. Runtime remains on its existing pin. Native/Provider or
Host/MCP/cloud activation, app installation, packaging, full workspace/frontend,
smoke and performance reruns are outside this slice. No private sources or
credentials were accessed and no manual project-status snapshot was updated.
