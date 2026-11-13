# ADR-MEM-143 — Retain distinct quoted ranges from one source

2026-11-13. A distilled statement can depend on two short passages in the same
message whose enclosing span exceeds the 500-scalar quote budget. ADR-MEM-142
refuses this input. Joining passages would invent a quote; dropping one loses
support, and changing source IDs would manufacture independent origins.

## Decision

Allow distinct Unicode-scalar ranges for one source only when AdmissionInput has
paired MemoryRecordFields quoted evidence. Keep the existing one-to-one order,
reference equality, exact text, speaker, source revision, locator and hash checks.
The (source ID, start, end) tuple must be unique. Inputs without quoted fields
still reject repeated source IDs. Each quote remains at most 500 scalars; source,
content, document and total evidence budgets are unchanged.

Validate each citation's anchor and subject attribution. Count origin uniqueness
once per source ID, so a second range is not an independent source. Distinct
source IDs with duplicate content or upstream message identity remain refused.
Dependency closure and sensitivity continue to use unique source revisions.
No adoption or knowledge state is upgraded merely by adding ranges.

The authored initializer retains every supplied support range and lets native
admission reject exact repeats. It does not concatenate quotes, demote support to
context, add sources, call a Provider, grant reads or create Owner confirmation.
Its hash-bound analysis and existing unknown-outcome barriers remain in force.

## Scope and verification

The wire shape is unchanged; acceptance of quoted inputs is extended. Runtime
must adopt the matching Memory revision and regenerated surface before writing
these records. Older evaluators may refuse them; no compatibility adapter is
added. This amends ADR-MEM-142's one-range restriction only.

Use synthetic separated passages, exact duplicate and unquoted-input refusals,
Unicode offsets, mismatched reference pairs, forged-range resolution, duplicate
origin regression checks and native write/reopen with one source dependency.
Production native use waits for Owner merge. Corpus completion, production
adoption, backup, deployment, deletion and remote read scope remain separate.

Validation: [same-source range checks](../validation/Same-source-quoted-ranges.md).
