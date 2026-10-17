# ADR-MEM-61 — Explicit collection shapes before offline assessment

Date: 2026-10-15. Scope: existing MV-7 offline quality evaluation.

The evaluator uses length, iteration and truthiness before confirming collection
types. A hash-bound response with an empty object/string in place of a changes
or summary array can produce a report. A mapping of supported paths can be
accepted as an observed unit's path list and retain supported_in_review.

Require a suite object, a cases array of objects and nonblank string identities
before uniqueness checks. Require changes/summary arrays, review arrays and
string path arrays for every unit status. Response items, judgments and evidence
entries must be objects; review evidence must be an array. These are input
shapes, not semantic grading or native response authentication.

Keep valid history-quality-1 inputs, canonical value hashes, unknown/pending
states, evidence quotes and metrics unchanged. Malformed input exits 1 through
the ordinary CLI diagnostic and emits no metrics, including when an earlier
case in the same suite was valid. Do not coerce objects, strings, booleans or
numbers into arrays or identifiers. Unknown metadata and mixed attachment
parts remain data and are preserved by the existing source binding.

No Rust, wire/storage schema, dependency, Runtime activation or stage boundary
changes. Real samples, Provider calls, model quality and owner acceptance remain
outside this repair. Evidence: [validation](../validation/MV-7-quality-collections.md).
