# MV-7 · Saved history batch response preview and application

Date: 2026-10-21 (Asia/Shanghai). Baseline
`18f4081d5fe8eef879d4383e306c24660f416c5e` (merged PR #39).
Scope: [ADR-MEM-72](../adr/072-history-batch-response-application.md).

## Behavior

Workspace IPC v1 adds read-only `history_batch_preview` and keyed synchronous
`history_batch_apply` (55 commands). Both resolve one existing child from a
saved admission task and its original selected batch index, revalidating the
full saved selection. The page cannot substitute a child Session, run, model,
source or actor. Preview returns the existing extraction preview with saved
response hash, proposals, exact quotes/selected text, application cursor and
current candidate state, plus task/batch/child identities. It writes nothing.

Apply requires the inspected response hash and an explicit cursor, binds the
complete request to a SessionAppend receipt, then reuses extraction governance
for at most one candidate. Its Provider helper compares the hash/cursor with
the job snapshot used for publication. A repeated older cursor returns current
verified progress without consuming the next item; a future cursor or changed
response conflicts. Reusing a successful/admitted key with changed arguments
conflicts across reopen. An admission receipt alone does not establish success:
pending-limit refusal can leave the request admitted with no candidate, and an
explicit original-key retry recovers after the blocker is resolved.

Existing candidate/cursor receipts, suppression, deduplication, pending bounds,
source permissions and pause/lifecycle controls remain authoritative. Empty
responses can complete the local job with no candidate. Applied suggestions
remain pending; Canonical memory requires the existing separate owner review.
Preview and application run without a configured native Provider. They neither
read credentials nor send requests.

## Verification

Four synthetic Workspace scenarios cover two-item preview and one-step
application, durable key binding/reopen, old-cursor replay under the original
and a new key, explicit next-cursor selection, current pending state, changed
hash/future cursor refusal without writes, unselected index, pause, source-read
revocation, lock, empty-response completion, and admission-only recovery after
a full pending queue. Saved responses are created by one explicitly confirmed
in-process fake exchange per scenario; the native Provider is then removed.
Port counters stay at one exchange throughout local application. These are
synthetic transport checks, not HTTP or real-account evidence.

Final checks used Rust 1.98.1 GNU with offline/locked Cargo, four test threads,
incremental compilation disabled and dev/test debug information disabled.
This retains the mainline verification concurrency; it does not diagnose or
claim a fix for the earlier default-parallel scheduling failures.

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 418 passed, zero failures, including all four new application scenarios; all doctest groups passed with zero doctests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Independent Python 3.12.10 / python-jsonschema 4.26.0 | Passed: 37 schemas, 56 valid records, 126 record cases, 5,496 set records, 51 IPC messages, 134 Workspace messages, 46 store documents, 8 Provider-invocation cases and 19 extraction-job cases |
| Runtime surface | Recorded and logged `c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`; 55 commands |
| Diff/public-content review | Passed; no added private files, personal absolute paths or credential-shaped strings |

Complete outputs and exit codes are retained in ignored
`.local/history-application-workspace.log`,
`.local/history-application-clippy.log` and their `.exit` files.
Interrupted earlier logs are retained with the `-interrupted.log` suffix and
are not passing evidence. The initial three-test focused run also passed before
the admission-recovery scenario was added and covered by the complete run.

## Remaining work

Runtime must adopt the exact Core pin and write catalog, expose response
inspection and explicit one-step application, and preserve response hash,
cursor and request/key ownership across ambiguous replies. Refresh current
candidate/review state rather than treating original model text as accepted
memory. Runtime UI/consumer/native acceptance, personal-history quality,
independent recovery, real Provider acceptance, semantic/temporal product
integration, MV-8 and cloud activation remain separate. Manual project status
snapshots and the retained MV-8 worktree were not changed.
