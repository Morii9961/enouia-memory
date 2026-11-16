# Fresh-run Owner backup compatibility — focused synthetic validation

2026-11-16. Based on merged fresh driver `442b22fa16881aae70f6f17c37b0c41464777ad6`
(PR109, ADR145), using the existing ADR136 export/restore APIs. This adds
regression coverage only; no production behavior, manifest, schema, Runtime
surface or dependency changes. All input and roots are synthetic.

## Coverage

`crates/enouia-memory-init/tests/fresh_backup.rs` loads an explicit fresh manifest
through `load_fresh`, packs two plans and enrolls the whole run through
`enqueue_fresh`. It supplies original shard bytes, exact source revisions and
quote ranges, authored content/provenance with unknown model identity, and an
explicit local attachment original. It never loads the owner's input archive.

- Completed run: form 65 records in two batches, logically delete one, release
  the writer and export. Make the original fixture root unavailable at its old
  path. Verify and restore using only the export; normal open and forward recovery
  remain quarantined until explicit synthetic activation. Check every exported
  file hash, byte-identical journal, complete persisted plans and ledger, exact
  original receipts, Raw/attachment closure and native source-quote resolution.
  The restored native read returns 64 visible memories and 65 sources; the deleted
  record remains hidden, no Owner action exists, and initializer model calls stay
  zero. The same run is accepted without work or a new commit; another run hash
  conflicts.
- Unknown result with an existing receipt: emulate interruption after the witnessed
  commit but before local receipt persistence. Export/restore the unknown journal;
  only the original receipt resolves it, with the same snapshot and packet/binding.
- Unknown result without a receipt: use the genuine native `prepare_formation`
  request/response/binding, persist the application intent, and emulate interruption
  before application. This is not a mismatched-binding fixture. After restore,
  repeated steps return `OutcomeUnknown`, preserving the entire original work
  item and creating no memory or commit.
- Expired formation authority: retain an already committed original operation but
  advance the fixture clock beyond its seven-day policy. After restore the original
  receipt remains withheld by existing native authorization; repeated steps keep
  `OutcomeUnknown` and the unchanged work item. No new key, retry or read grant.

## Checks actually run

- `cargo test -p enouia-memory-init --test fresh_backup --locked -- --nocapture`:
  **2 passed**, no ignored/filtered tests. The second test covers all three unknown
  result cases above. Execution took 144.64 seconds.
- `cargo clippy -p enouia-memory-init --test fresh_backup --no-deps --locked -- -D warnings`: **passed**.
- Targeted rustfmt and `git diff --check`: **passed**.

## Limits and next gate

This is a Windows local synthetic round trip, not a copy on independent encrypted
media or a real-data disaster-recovery result. It does not authorize deletion,
activation of a real restored root, migration, deployment, Runtime adoption or
long-term reads. The real source/archive and native Owner data are untouched.

The test uses existing native library APIs and a synthetic descriptor. It does
not prove that the fresh production CLI can enroll a relocated/restored root:
ADR110's OS-account/canonical-home bootstrap registration remains outside the
ADR136 export and is not replaced by a descriptor or restore receipt. Native
root-opening authority for an actual restored installation must be verified in
its own authorized acceptance, preserving the original operation and unknown
barriers. Existing Owner restore quarantine and stop-old-Host requirements remain.

After ADR146 merged as main `a543c1e`, this branch integrated that exact main
without changing the regression fixture. The same focused `fresh_backup` target
was rerun: **2 passed**, no ignored/filtered tests, 25.54 seconds. This establishes
combined local compatibility for these synthetic inputs, not real corpus capacity
or independently encrypted media. No ADR146 implementation was rewritten.
No full-workspace, Linux, schema, Runtime surface or independent-media checks
were run for this test-only change.
