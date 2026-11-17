# Grouped support origins — synthetic validation

2026-11-17. [ADR-MEM-149](../adr/149-grouped-support-origins.md), based on merged
main `d0e7f9f0298e8b5c4c178dc2649a647c28e8596e`. All implementation checks below
use synthetic inputs. Subsequently merged in #117 at `17a369e`; its rebuilt
formal CLI completed a real read-only whole-run preflight. Native admission
passed, while journal capacity refused. There was no real formation. That
separate capacity gate does not change the synthetic evidence reported here.

## Behavior

A statement quoting two original messages with identical text previously reached
`fresh_journal_capacity` and failed with `next.duplicate_origin`. The new synthetic
regression reproduced that exact refusal before implementation.

`AdmissionInput.support_origin_groups` now explicitly retains collision components
as sorted exact source revisions. Native evaluation recomputes the components
from trusted content hashes and upstream identities. Every occurrence keeps its
original source and quote; each component represents one support origin. No
knowledge/adoption outcome, source gate, evidence limit or confidence changes.
Ungrouped copies still fail, and legacy inputs serialize with the field omitted.

The authored initializer supplies the groups without changing manifest bytes.
Native replay validates them again. Older closed readers can reject these inputs;
Runtime must adopt the new exact pin/surface before opening grouped decisions.
Workspace stays at 61 commands. No Runtime checkout changes here.

## Checks actually run

- Contract `next_admission`: **18 passed**. Includes legacy serialization and
  copy refusal, equal-content distinct occurrences, equal-upstream copies,
  transitive components and permutation-stable grouping. Missing, stale,
  reversed, overlapping, incomplete and unrelated groups fail. Every occurrence
  retains availability and clear-self-report speaker checks; outcomes stay equal.
- Initializer `fresh_driver` and `packing`: **16 passed, 1 ignored**. The new
  native regression forms, closes/reopens, reads both exact original occurrences,
  verifies Raw/message identities, checks journal lifecycle bounds and resumes
  the same run without another commit. Existing title/visible-text/context,
  original-analysis, uncertainty, multibatch and overflow cases pass.
- A subsequent focused boundary case: **1 passed**. Keeps 128 exact quotes over
  64 distinct same-text source occurrences in one origin component; the next
  quote still trips the existing native evidence bound.
- Host `automatic_loop`: **69 passed**. Vault `formation_title`, `segmented` and
  `segmented_backup`: **1 + 4 + 5 passed**, including native replay, recovery,
  semantic backup verification and deletion/policy barriers.
- Contract `runtime_surface`: **4 passed**. Python surface regeneration/check
  records aggregate `df3248e725f92952d0be265092babd5dbdbe6fec642720a0c400c71cbd9e7e53`
  and 61 commands; adoption requirements are logged in `RUNTIME.md`.
- Independent Python schema cross-check with existing pinned jsonschema 4.26.0:
  **79 schemas and the full fixture manifests passed**. The system Python lacked
  jsonschema; the existing private validation environment supplied the pinned
  checker without installing anything. An initial Rust fixture dispatch failure
  was corrected to recognize the new grouped-input fixture; the final 18 tests
  above passed.
- All-targets Clippy for contract, Vault and initializer with `--no-deps` and
  `-D warnings`, workspace formatting and `git diff --check`: **passed**.

No full-workspace suite, Linux run, real input on the new implementation, Provider,
OwnerReadGrant, production formation, deployment or data deletion was performed.
Real whole-journal capacity and backup/restore acceptance remain separate gates.
