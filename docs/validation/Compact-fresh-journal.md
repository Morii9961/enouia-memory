# Compact fresh journal — synthetic validation

2026-11-17. [ADR-MEM-150](../adr/150-compact-fresh-initialization-journal.md), based
on merged `17a369e010f670baacaeec98306854fd4d14afc5`. This is synthetic evidence;
real use of this capability awaits Morii-approved merge and rebuilt formal tools.

## Representation and retained bindings

New fresh runs store each initial Plan in a content-addressed Raw object bound to
the run hash, Vault/descriptor and plan position/count. Journal version 2 keeps
the object hashes, capture state and every original request, response, generation
binding, receipt and work state. Completed and uncertain items remain present.
Each plan object and the entire mutable lifecycle still have a 16,777,216-byte
limit. Read-only preflight reports both bounds and the previous inline sizes.

Open verifies object bytes/hashes and exact bindings before rebuilding Plans.
It refuses missing, corrupt, wrong-run and wrong-position objects without repair.
Old inline version-1 journals reopen and save in their original representation;
there is no automatic migration. Unknown results retain the original full packet
and only resolve its existing receipt. Policy expiry can still withhold lookup.

Morii backup already includes all Raw objects and the unchanged private journal
path. The initializer verifies those plan objects again on restored open. Native
admission, memory decisions, Runtime readers and Workspace commands do not change.
An old initializer cannot resume version-2 journals; use the compatible fresh
binary. ADR149's reader upgrade requirement for grouped decisions still applies.

## Checks actually run

- `fresh_driver`: **15 passed, 1 opt-in initially ignored**. Covers large immutable
  Plans, actual enqueue/capture/formation, close/reopen, isolated backup/verify/
  restore/activation, original receipt and byte-identical journal retention,
  corruption/binding refusals, historical inline continuation, exact source reads,
  uncertainty, original analysis, source groups and same-run idempotency.
- `fresh_backup`: **2 passed, 1 opt-in initially ignored**. Retains all multibatch
  evidence, Raw, deletion barriers and original uncertain packets through restore.
- Both opt-in production CLI tests were then explicitly run under the actual
  Windows account in a protected, marked synthetic-only durable parent:
  **1 bootstrap/formation/reopen + 1 activated-restore continuation passed**.
  Tests retain their synthetic homes and use no existing Morii library.
- The remaining initializer library, binary and integration targets:
  **58 passed** across captured formation, parser/selection, initialization,
  legacy continuation and packing. Total for the changed crate: **77 passed**,
  including the two opt-in tests; no failed or unexecuted opt-in acceptance remains.
- Initializer all-targets Clippy with `--no-deps --locked -- -D warnings`,
  workspace formatting and `git diff --check`: **passed**.

Synthetic size evidence (bytes):

| Case | Inline queued | Inline lifecycle | Mutable queued | Mutable lifecycle | Largest plan object | Result |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1,024 records, 8 Plans, long bounded sources | 28,506,882 | 39,841,194 | 163,258 | 11,497,570 | 3,482,650 | Pass; total plan objects 27,860,088 |
| 65 records, two Plans | — | — | 10,836 | 730,021 | — | Actual completed journal 722,863 |
| 512 large records | — | — | 81,762 | 21,076,102 | — | Refused before archive/capture |

Before implementation, the large synthetic case failed the whole-journal bound.
Its first fixture used multibyte text beyond the existing per-source byte limit;
the corrected ASCII fixture stayed within that limit and reproduced the intended
whole-run capacity failure. Neither limit was changed to make the case pass.

No covered Runtime surface file or JSON Schema/contract validator changed, so
schema and integration checkers were not rerun. The existing aggregate remains
`df3248e725f92952d0be265092babd5dbdbe6fec642720a0c400c71cbd9e7e53` with 61 commands.
No full-workspace suite or Linux run was performed. No real input on this new
implementation, production Vault, Provider, grant, Runtime checkout, installation,
deployment or data deletion was part of these checks.
