# MV-7 private seed staging and Runtime handoff

2026-10-18. This owner-authorized slice connects an existing privately curated
portable bootstrap to the native Vault and Runtime review workflow. It remains
within MV-7; it does not start MV-8, cloud hosting or automatic approval.

This records the first staging checkpoint. The subsequent
[context review and episode correction](MV-7-seed-review-context.md) records
the additive Core results, current full-suite pass and later private revisions.

## Implementation

The [preparer and runbook](../../tools/seed-stage/README.md) verify the selected
originals and produce derivative ChatGPT subsets, imported provenance documents
and an exact-package staging manifest. The CLI adds `seed-preview`, `seed-stage`
and `seed-review-check`. Native imports precede agent proposals; receipts bind
the frozen package and seed revision. No new IPC, record schema, Core API,
Provider call or Runtime dependency is introduced.

The CLI uses the workspace's existing pinned serde dependency for strict
package structs; the boundary allowlist and lockfile record that addition.
Runtime's recorded Core revision already supports the candidate context
dependency field. The [integration handoff](../integration/RUNTIME.md#private-seed-staging)
describes existing host behavior rather than requiring a pin bump.

## Private migration evidence

The selected real dataset produced 185 pending candidates, including 32 core
records, associated with 532 selected messages and 21 reviewed supporting
originals. The complete original archive and unreviewed attachments remain in
the private bootstrap. These counts do not assert full native archive import,
independent fact verification, complete attachment review or owner acceptance.

The first local staging run exposed a converter error: native subject/item
identifiers were generated as UUID v5, whereas the contract requires v4. All
185 candidates were corrected through ordinary proposer edit transactions,
retaining candidate IDs, sources and prior revisions. The preparer now emits
v4 IDs, and CLI preflight rejects malformed typed IDs before import. After the
repair, all 185 acceptance plans built successfully without confirmation;
Canonical and owner-review record counts remained zero. The frozen first-run
package and repair receipts are private historical artifacts; its invalid IDs
are deliberately rejected by the corrected preflight.

The new Vault has owner-only permissions. A pinned local checkpoint was
exported, independently verified as a package, restored to a separate inactive
directory and verified clean at the same commit: 24 objects and 1,325 record
revisions checked. The restored copy retains the native network-reconciliation
barrier. The original Vault is the only intended active primary.

Both local disk letters resolve to the same physical disk. This checkpoint is
unencrypted local recovery evidence, not an independent encrypted backup.
An initially selected AppData path was refused by native canonical-path policy
because desktop application virtualization changed its physical location; the
completed drill used a non-virtualized private directory instead. No policy
was bypassed.

The existing Runtime executable was launched with the explicitly selected new
Vault; a read-only exclusive-open probe confirmed its native host lock was
held by another process after launch. Native window inspection was requested; application access approval
timed out. Actual rendered inbox/source acceptance is therefore pending unless
the owner subsequently authorizes and completes that inspection. A process
launch and a source pin record do not prove frontend acceptance.

## Verification

Executed with Rust 1.98.1 and offline dependencies:

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked` | 382 passed, one workspace race test timed out; not a clean full-suite pass |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| Python 3.12 / pinned jsonschema cross-check | Passed: 36 schemas |
| `python tools/integration/runtime_surface.py` | Passed; existing logged aggregate unchanged |
| Preparatory Python tests | Four synthetic cases passed |
| Final CLI bridge integration test | Passed, including native review/search, replay, input changes and v5 rejection |
| Isolated rerun of the timed-out workspace race | Passed in 0.62 seconds; does not establish full-suite stability |

The failed test was
`full_session_ask_replays_an_input_published_while_the_call_is_paused`.
Its saved phase trace showed first release at 58 ms, finish timeout at 5,069 ms,
and first return at 6,764 ms. The existing 5-second ordered barrier expired
before the first call returned. Production Workspace code, race synchronization
and deadlines were not changed in this slice. The earlier snapshot already
tracked unexplained workspace timing failures; this new trace is additional
evidence, not a scheduling fix.

Initial sandbox runs of native Vault creation returned AccessDenied; the same
isolated synthetic scenario passed outside the sandbox. Python tests also
exposed an unclosed SQLite read connection during cleanup, which was corrected
by explicitly closing it. These environmental and converter failures are
recorded rather than counted as successful checks.

## Remaining owner actions and limits

Review the exact candidates in Runtime before creating Canonical memories.
Portable default-recall annotations do not silently become native compiler
policy; sensitive/restricted entries can remain pending. A valid plan is not
approval. Source notes, model context and copied summaries remain evidence
bounded, and provider egress remains a separate owner decision.

Complete real frontend acceptance and choose an independent encrypted recovery
target before treating this as a daily-use or disaster-recovery acceptance.
No real content, database, attachment, receipt or recovery material belongs in
Git. Public documentation retains aggregate evidence only.
