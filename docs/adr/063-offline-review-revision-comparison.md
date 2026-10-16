# ADR-MEM-63 — Read-only comparison of offline review revisions

Date: 2026-10-16. Scope: current MV-7 offline review workflow, following
[ADR-MEM-62](062-offline-quality-review-worksheet.md).

## Decision

Add an explicitly selected before/after suite comparison. Validate each complete
suite with the existing evaluator and bind the snapshot used for comparison to
its evaluated JSON-value hash. Require the same case identities and all input
fields to match. Only reviewer, item_judgments and unit_judgments may change;
sources, responses, required units, native declarations and extra metadata remain
bound. Compare canonical JSON digests so distinct JSON types stay distinct.

Case ordering and judgment ordering may change without becoming judgment edits.
Match cases by their existing unique identity, item judgments by response path,
and unit judgments by the fixed required-unit positions. Report added, edited
and removed judgment locations alongside before/after counts and review states.
Reviewer declarations may change only as permitted by the existing evaluator;
the comparison does not authenticate reviewers or call a removal an improvement.

Return bounded-by-schema, UTF-8 JSON only after the entire comparison succeeds.
Output contains case positions, input hashes, fixed paths, numeric unit positions,
declarations and counts; do not repeat case names, required-unit text, sources,
responses, evidence quotes, reasons or saved analysis input. Hashes/positions are
review metadata and do not make a report public or anonymous. Keep
native_validation=not_rerun and overall_model_quality=null.

Reject altered input or any invalid suite as a whole, leaving stdout empty.
There is no save, merge, patch, Markdown import, automatic judgment, Provider call,
Vault access, memory approval, Runtime UI or wire/storage change.

## Evidence

Implementation and synthetic validation are recorded in
[the comparison report](../validation/MV-7-review-revision-comparison.md).
Real review quality and owner acceptance remain separate.
