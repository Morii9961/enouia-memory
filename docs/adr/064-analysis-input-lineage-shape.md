# ADR-MEM-64 — Shared saved-analysis lineage shape check

Date: 2026-10-16. Scope: current MV-7 offline quality input validation.

## Decision

Use the existing synthetic native-matrix lineage shape gate for optional
selected_lineage in every saved complete analysis input. Move the gate to the
evaluator and reuse it from the matrix checker. Validate the complete input's
hash and selected scope as before; if lineage is present it must have one row
per selected source, exact row keys, complete unique integer source indices,
unique in-range integer parent indices, and a nonnegative integer or null
unselected_parent_count. Booleans are not integer indices or counts.

Omission stays valid for legacy/plain tasks. Unknown parent counts remain null;
do not fill them with zero or infer/fetch edges. The gate validates structure,
not ancestry, submitter identity, original native execution, or model quality.
Keep native_validation=not_rerun and overall_model_quality=null.

This narrows malformed offline input acceptance only. It does not change Rust
producers, wire/storage contracts, scoring of valid suites, Runtime adoption or
owner authority. All consumers of the evaluator reject the entire invalid suite
before emitting output. No writes, Provider calls, new stage or activation.

## Evidence

[Validation record](../validation/MV-7-analysis-lineage-shape.md).
