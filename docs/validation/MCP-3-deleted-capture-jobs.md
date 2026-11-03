# Captured formation: deleted-source settlement

2026-11-03. Base `0d338f1`. This corrects the deleted-source gap recorded in
[the worker report](MCP-3-captured-worker-host.md), under ADR-MEM-112.

Deleted sources drop out of the dispatch queue. Previously, an already analyzed
job therefore stayed `forming` forever. The worker now advances its durable
remaining work separately: unpublished statements count as refused, interrupted
applies recover actual receipts from the chain, and the ordinary terminal states
remain `rejected` or `formed`. No analysis is repeated and no content is published
after deletion. Existing policy-gap holds are preserved.

Owned files: Vault `automation/segmented/formation_worker.rs`, initializer
`tests/captured_formation.rs`, ADR112, the captured queue contract, this report,
Runtime integration log and generated manifest. No dependency, schema, public
method, journal version or wire enum changes.

Actual checks on the final Rust source:

- `cargo test -p enouia-memory-init --test captured_formation --locked --no-fail-fast`:
  **13 passed, 0 failed**, serial, offline, pinned Rust 1.98.1.
- Deletion after analysis, preparation and a successful apply reaches a terminal
  state; later turns form; reopening preserves statuses with no second analysis.
- Both StagingWritten and AfterCurrent publication faults recover before deletion.
  Settlement preserves exactly the chain-proven receipt count, publishes no new
  commit after deletion, and remains stable across another reopen.
- Existing concurrent policy/apply, policy-gap, refusal and unknown-analysis
  regressions pass in that same target.
- `cargo fmt --all -- --check`, `git diff --check` and the regenerated Runtime
  surface check passed. Aggregate:
  `825d6854edbdc0bb567364ba55d263291683185acd1c76b8ebdd527534938c05`.

The first sandboxed test run failed all 13 tests while creating their synthetic
roots with StorageFailed, before worker behavior ran. The same command passed
under ordinary Windows permissions. All data was newly generated synthetic
material. No private root, real analyzer or Runtime instance was used.

No workspace suite, Clippy or schema cross-check was run for this localized fix.
Pending analysis still follows its existing completion/deadline behavior; prompt
cancellation after source deletion is a separate follow-up. Terminal journal
capacity, broader context, real model quality and Runtime adoption remain open.
