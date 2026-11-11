# Synthetic Owner portability rehearsal (ADR-MEM-138)

2026-11-11. Based on main `2ed930f` after PR98 and PR99 merged. The harness is
`crates/enouia-memory-init/examples/portable_owner_backup.rs`.

## Procedure

```text
cargo run -p enouia-memory-init --example portable_owner_backup --locked -- export
cargo run -p enouia-memory-init --example portable_owner_backup --locked -- restore <SYNTHETIC_PACKAGE>
```

Export takes no source path. It uses the built-in display suite, forms its
records, deletes one memory, saves native observations and exports the stopped
root. The old source path is made unavailable; every synthetic artifact is
retained outside repositories. Restore always allocates a fresh temporary root.
It verifies the package, checks quarantine, explicitly activates the synthetic
copy, and compares the snapshot, ledger, memory records/receipts, sources and
conversations. Every visible quoted source jump must resolve. Completed
initialization must do no new work and leave the snapshot unchanged.

For Linux, transfer only `export/` and `expectations.json` to an empty directory
on the Linux filesystem. Do not copy the source root or its indexes.

## Windows evidence

- Focused example build and clippy (`-D warnings`) passed; formatting passed.
- Export: 48 files, commit sequence 41.
- Native restore: 36 sources and 36 visible memories; exact ledger and native
  observations match; source jumps resolve; deleted memory stays unavailable;
  no new commit, no model invocation.

## Cross-platform evidence and limits

The same 48-file Windows export and its expectations were copied to the Linux
filesystem under WSL Ubuntu-24.04, without the source root or indexes. A detached
worktree at implementation commit `1248212` built and ran the restore harness:
36 sources, 36 visible memories, equal ledger and native observations, resolvable
source jumps, preserved deletion barrier, zero new commits and model invocations.
The first copy attempt stopped on Windows/WSL argument quoting before restoring;
the corrected transfer ran successfully without changing the package.

Existing platform-native backup tests are recorded separately in
[ADR136 evidence](Segmented-owner-backup.md).

This is not a real-data initialization, encrypted independent-media backup,
VPS deployment, Runtime acceptance or ChatGPT connector test. Production APIs,
schemas, dependencies and the Runtime integration surface are unchanged. No full
workspace suite or new MV stage was started.
