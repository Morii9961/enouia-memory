# ADR-MEM-58 — Selected-source lineage input

Date: 2026-10-13. Scope: MV-7-H2i, optional native analysis input.

The semantic task asks the model to distinguish corrections from divergent
branches, but its original input exposes no direct parent edges. Add the opt-in
history_semantic::create_with_lineage helper. Keep create, legacy task bytes,
request derivation and reopening unchanged. Separate session/input keys and
Request IDs for the new helper, even when the owner supplies the same operation
key. Output remains history-semantic-review-1; no wire/storage schema changes.

The new Request ID hashes a domain longer than the maximum 256-byte owner key,
followed by its namespace and key hash. A legacy key can imitate a textual
namespace, so the namespace alone cannot separate the two hash-input domains.
Legacy derivation stays unchanged. This corrects the unmerged H2i helper;
synthetic tasks created with its earlier derivation must be recreated with a
fresh owner key, never silently rebound or resent.

selected_lineage has one row per selected source span: source_index,
parent_source_indices and nullable unselected_parent_count. Read only the selected
Source revisions at the reviewed pin. Emit direct edges to selected spans; do not
fetch missing parents or emit their IDs/content. Zero missing parents differs from
unknown parent metadata (null). A gap does not prove there is no ancestor path.
Repeated spans of a parent source can have multiple indices. Source text, branch
IDs and timestamps cannot invent edges, current truth or business effective time.

Include metadata and its instruction in the immutable task hash and existing
64 KiB whole-task budget; reject overflow rather than truncate. Recompute the
complete selected-source plan before preparation, guard delivery and preview.
The existing native validator still checks actual selected SourceRecords, exact
quotes and user evidence; model-declared lineage never replaces native checks.

No send, broader source selection, proposal or approval is authorized by creating
either task. Use normal exact-body inspection, AnalysisGuard, quota and permission
checks. Reopening never resends. Runtime may opt in at a new pinned revision and
must inspect the new request separately; an old request approval is insufficient.
Quality bundles keep the native input hash; inspecting the complete analysis input
is still necessary to review the additional lineage supplied to the model.

All validation is synthetic. Real model quality, Runtime product adoption and
owner acceptance remain pending; MV-8 is not started.
