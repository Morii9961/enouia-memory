# Fresh cross-batch source identity

2026-11-18. [ADR-MEM-151](../adr/151-fresh-cross-batch-source-identity.md).
Synthetic inputs only, based on `506d23dc174a3e927d323f8f8d0b8ecfc9d1726d`.

Packing two statements into separate plans reproduced the old capture denial.
Both plans passed read-only admission and capacity, then the second plan tried
to capture an existing locator with a second SourceId. The native uniqueness
check rejected it after the second policy was published.

Fresh preparation now reuses the first exact source revision across batches.
The first capture owns the source and a later batch's policy covers precisely
its listed dependencies. Exact evidence, context and entity references resolve
after reopen. The stored native contract and original range boundaries stay
unchanged. The new trusted CLI also reconciles the specific stopped-run shape
described in ADR151. It retains the first plan, original request and receipt,
original journal, and original run hash. The revised pending policy is posted
by ordinary `run` after reconciliation.

Checks performed on Windows with the pinned Rust toolchain:

- `fresh_driver`, `fresh_backup` and `packing` passed **24** tests. Reconciliation
  rejects a stale journal and an unresolved outcome. It preserves the first
  compact plan entry and archives the old journal. A synthetic export, restore
  and activation before resumption preserves the original receipt and allows
  the second batch to complete. Cross-batch formation, closure and reopen
  keep 65 unique source identities for two records. Lifecycle bounds and older
  inline compatibility still pass.
- Two existing opt-in native CLI tests passed with protected synthetic homes.
- In a separate actual-account synthetic rehearsal, the old release CLI was
  invoked first and reproduced `fresh.step: Denied`. The candidate CLI's
  `reconcile-source-overlap` retained one receipt without moving CURRENT.
  `run` completed the second record with the original run hash, and a repeated
  `run` left CURRENT unchanged.
- Scoped all-targets initializer Clippy with `-D warnings`, formatting, diff
  and public-content review passed.

No covered Runtime surface file or JSON schema changed. The existing surface
aggregate `df3248e725f92952d0be265092babd5dbdbe6fec642720a0c400c71cbd9e7e53`
and 61 commands still describe compatibility. No real data was used with this
implementation. Production formation and recovery require separate evidence.
