# MV-8.1g · Host-bound Workspace service

Date: 2026-10-22 (Asia/Shanghai). Baseline
`450abcd4f181c898e60231fdbaa98c7a762de667` (merged PR #44).
Continues owner-authorized MV-8 with synthetic local roots only.

[ADR-MEM-76](../adr/076-host-bound-workspace-service.md) connects the real
Workspace to the native writer and session gate. The service binds an existing
v2 Vault, forwards the unchanged 55-command envelope after scoped admission
and fresh authority verification, and retains the same OS lease across UI
lock/reopen. Root changes/unlock remain trusted native controls. Embedded
clients still use their original constructor and lifecycle.

Four native Windows integration tests passed:

- Real Workspace remember creates a pending proposal; owner plan/confirm
  creates Canonical memory; QuickSearch reads it but cannot write.
- UI lock revokes old connections while competing coordinators/services remain
  busy. Native owner unlock requires a new connection; the original remember
  key replays the same candidate without another CURRENT publication, including
  after service drop/reopen. QuickSearch cannot invoke native owner unlock.
- Page root open/create/unlock is refused. A corrupted witness blocks Core
  replay and new admission before any content/receipt response or data write.
- Ordinary roots and unbound Vault handles never acquire Host mode through
  service/Workspace construction; CURRENT and adoption intent remain unchanged.

The initial fixture used a too-short IPC idempotency key and was correctly
refused by the session parser. Updating only the synthetic key to the existing
16–128 byte rule made the integrated behavior run. All seven existing session
tests also passed. No product validator was weakened.

Final checks:

| Check | Result |
| --- | --- |
| `cargo test --workspace --locked` | 472 passed, zero failures; doctest groups passed with zero doctests |
| Strict workspace all-target Clippy | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface checker | Passed; recorded/logged `dc95e3d82770167bfdcadfe622fdd00964848a614c70a5a163b1a27c93bbbddc`, 55 commands |
| Public-content/diff review | 13 engineering/documentation/synthetic files; local links resolve, no matching private paths or credential shapes; instructions/status excluded |

Logs and exit codes are retained in ignored `.local/mv8-service-*.log`/`.exit`.
Checks use pinned Rust 1.98.1 GNU, offline/locked Cargo and four test threads,
with incremental compilation/dev-test debug information disabled. Schema and
validator code are unchanged; the preceding independent 43-schema evidence
is reused, without rerunning that unrelated check.

This proves a native library service over synthetic Windows storage, not a
running user-level executable, protected listener, actual OS authentication,
reference-shell/Runtime transport adoption or real-client acceptance. The
native adapter still authenticates owner processes/windows before minting
admission; same-user access or a JSON principal does not suffice. Native picker
and Provider setup remain absent here. Existing Core async lifecycle/approval
rules still apply; reconnect never sends or repeats an external invocation.
Physical purge/export, independent encrypted restore, real Provider quality,
private-data migration and cloud activation remain separate gates. No private
material, credentials, real model calls, Runtime checkout or manual status
snapshots were used or changed.
