# MV-8.1 · Authority foundation integration

Date: 2026-10-21 (Asia/Shanghai). Baseline
`9d68d3d8707749ad3b0e2a36cfc97d71a27e712f` (merged PR #40).
Continues the previously owner-authorized MV-8.1a–c work, without real
migration, credentials, Provider requests or cloud activation.

## Integration

The retained isolated `codex/mv8-host-contracts` worktree contained uncommitted
authority contracts, authority-only control persistence, and shared CURRENT
publication on an older baseline. This branch copies only its inventoried
engineering/documentation/synthetic files and merges them with the current
MV-7 implementation. The original worktree remains unchanged. Four additive
conflicts retained both the new history module/API and Host module, both ADR
sets, the history-admission constraints and the Host constraints, and all
historical Runtime compatibility entries. No root user instructions, private
materials or manual project snapshots are included.

[ADR-MEM-66](../adr/066-host-authority-and-platform-contracts.md) freezes bounded
authority/transfer documents and native transaction obligations.
[ADR-MEM-67](../adr/067-authority-control-file-persistence.md) supplies the
explicit authority-only Windows pilot with owner ACL and process locks.
[ADR-MEM-68](../adr/068-shared-current-authority-publication.md) supplies opt-in
HostCoordinator publication: immutable authority and data become visible by
the same CURRENT replacement, native identity/quiescence are rechecked before
replay or writes, and actual pinned closure bytes are verified on every
operation. A persistent marker refuses legacy writes, recovery adoption,
purge/sweep and lossy export even after the coordinator exits. Legacy roots
omit the optional CURRENT field and retain their serialization.

The integration preserves the existing 55-command Workspace contract and all
history planning/materialization/preparation/application code. Existing
embedded roots do not automatically enter Host mode. CURRENT's populated
optional field needs matching strict readers; older binaries are unsupported
for Host roots. The pilot and coordinator are separate native library APIs;
neither is an authenticated listener or a Runtime transport switch.

## Verification

Focused current-branch tests passed 26/26: 10 pure contract tests, 8 native
control-file/process tests and 8 data/authority transaction tests. They cover
exact promotion replay, monotonicity/receipt budgets, unavailable/corrupt state,
owner isolation and competing OS leases, cached-file corruption, persistent
legacy/export fencing, partial bootstrap refusal and actual child-process
abort before/after publication. Process-abort fixtures terminate only their
own synthetic child; they are not power-loss evidence.

The independent pinned Python 3.12.10/python-jsonschema 4.26.0 cross-check passed:
39 schemas, 56 valid records, 126 record cases, 5,496 set records, 51 IPC
messages, 134 Workspace messages, 49 store documents, 19 Host documents,
8 Provider-invocation and 19 extraction-job cases.

Final checks used pinned Rust 1.98.1 GNU, offline/locked Cargo and four test
threads, with incremental compilation and dev/test debug information disabled:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 444 passed, zero failures; all doctest groups passed with zero doctests |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Independent pinned Schema cross-check | Passed with the counts above |
| Runtime surface | Recorded/logged `81c671c95bd42e0845b276459b6252f181dbe26cc5ea43941bdd38f361bad8af`; 55 commands |
| Public-content/diff review | Passed: 43 engineering/documentation/synthetic files, local links, no new private paths or credential-shaped strings; original 42-file worktree snapshot preserved and manual status unchanged |

Complete output and exit codes are retained in ignored
`.local/mv8-continuation-{focused,workspace,clippy}.log` and `.exit` files.

Earlier isolated reports preserve their failed ordered-barrier workspace run
and the subsequent targeted passes. They are historical evidence, not current
integration acceptance. This passing current four-thread run does not diagnose
or prove a fix for their default-parallel scheduling failures.

## Remaining MV-8 gates

Partial adoption or damaged CURRENT remains fenced; an explicit Host-aware
repair/restore design is still required. The data checkpoint excludes mutable
audit/journal, credentials and platform metadata and does not prove independent
target restoration. Native caller identity and owner approval remain trusted
control-plane duties. A copied local authority is not distributed consensus.

Next remain Host-aware recovery, the unique user-level Host process,
authenticated sessions/version handshake/revocation and reference-shell
disconnect/reconnect; restricted MCP and concrete client compatibility follow.
Runtime must separately adopt and verify the Host before retiring its embedded
writer. Real Provider P02–P06, personal-history quality, independent encrypted
restore, production migration and cloud operation remain separate gates.
