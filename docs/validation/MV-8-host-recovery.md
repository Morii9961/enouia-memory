# MV-8.1d · Witnessed Host publication and explicit forward recovery

Date: 2026-10-22 (Asia/Shanghai). Baseline
`13aeb3871e9353df6a74c5674dbc7d26fcb6ce7f` (merged PR #41).
Continues owner-authorized MV-8 with synthetic local Vaults only.

## Result

[ADR-MEM-73](../adr/073-host-publication-recovery.md) adds an opt-in native
`host-current-v2` protocol. Before CURRENT replacement, a flushed bounded
intent selects the exact before/after pointers and after-authority. A flushed
witness acknowledges that publication before success is returned. Normal Host
operations require all three to agree; an interrupted publication blocks
writes, including the original key's replay, until explicitly recovered.
Initial intent presence fences legacy APIs before a mode marker exists.

Native `HostRecovery` holds the same OS lease, verifies the actual target data
closure and legal authority transition, then produces a read-only plan. Owner
confirmation binds its exact hash, native after-primary identity, intent and
observed CURRENT/mode/witness/after-authority bytes. Only the latest verified
target may be finished. It never chooses an older commit, changes content,
lowers watermarks, creates another commit or loses the original receipts.
Completed repair replays without writing; a repair interrupted after CURRENT
can continue under the original confirmed plan when the observed changes are
its exact target bytes.

Missing/torn mode or selected after-authority metadata can be atomically
restored from the intent. Missing ordinary witness, corrupt intent/witness,
missing/corrupt before-authority, damaged data, foreign native identity or a
valid unrelated CURRENT refuses recovery. Prototype v1 roots remain fenced,
with no implicit migration or initialization. Ordinary embedded roots and
the 55-command Workspace envelope are unchanged. This native API is not a
page/MCP capability or an automatically approved recovery action.

## Evidence

Eight focused native recovery tests passed. They cover bootstrap interruption
at intent/before-CURRENT/after-CURRENT/witness boundaries, exact confirmation,
read-only planning, corrupt CURRENT without watermark rollback, unavailable
proof, changed observations, missing authority reconstruction, damaged data,
repair lost replies, interrupted promotion and persistent old-primary fences,
prototype v1 refusal, and partial new mode/authority files. The existing
transaction suite also aborts its own synthetic child process at the four
publication boundaries, then recovers the staged commit and replays its key
without a second authority advance. These are process/I/O tests, not power-loss
or whole-filesystem rollback evidence.

The partial-file regression first failed: `write_new` creates a file before
the injected WriteBytes failure, so treating only absence as recoverable left
bootstrap unavailable. The fix includes actual mode/after-authority bytes in
the confirmation plan and restores only the verified target atomically.
Changed partial bytes after planning now conflict; a fresh inspection binds a
new plan. The before-fix failure remains in ignored
`.local/mv8-recovery-partial-metadata-before.log`.

Two additional pure contract tests passed for legal bootstrap/data/control
successors, illegal successor refusal, duplicate decoded fields and document
budgets. The independent pinned Python 3.12.10/python-jsonschema 4.26.0 check
passed: 41 schemas, 56 valid records, 126 record cases, 5,496 set records,
51 IPC messages, 134 Workspace messages, 49 store documents, 29 Host documents,
8 Provider-invocation and 19 extraction-job cases.

Final checks use pinned Rust 1.98.1 GNU, offline/locked Cargo, four test
threads, disabled incremental compilation and dev/test debug information.
Final results:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 454 passed, zero failures; doctest groups passed with zero doctests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Independent Schema cross-check | Passed with the counts above |
| Runtime surface checker | Passed, recorded/logged `5fa75d96ff9aa1c20e1cdbf729975624bb67f0b51d704058d8ccfb917908701b`; 55 commands |
| Public-content/diff review | 25 engineering/documentation/synthetic files; no matching private paths, credential shapes or broken local links; user instructions and manual status excluded |

Logs and exit codes live in ignored
`.local/mv8-recovery-{targeted,workspace,clippy,schemas}-final.*`.
An earlier full run was deliberately interrupted after the metadata failure
was reproduced; `.local/mv8-recovery-workspace-before-metadata-fix.log` is not a
passing result. Historical ordered-barrier failures are not erased or proved
fixed by a passing four-thread run.

## Next gates

The unique user-level Host process, native authenticated client sessions,
version handshake, revocation and reference-shell disconnect/reconnect remain
next. Domain writes must first use the Host coordinator rather than the
persistently fenced legacy writer. Restricted MCP and concrete client checks
follow that foundation. Runtime adoption remains a separate pinned integration.

Independent encrypted restore, actual target restoration, external rollback
of the entire filesystem, distributed authority, real Provider P02–P06,
personal-history quality, owner acceptance, real migration and cloud activation
are not established by these local library checks. No private memories,
credentials, Provider sends, Runtime checkout or manual status snapshots were
used or changed.
