# Linux Owner bootstrap — validation (2026-11-09)

Scope: [ADR-MEM-134](../adr/134-linux-owner-bootstrap.md), branch
`claude/linux-owner-bootstrap`, stacked on `claude/remote-mcp-service`
(PR #92), which lets segmented Vaults open on Linux.

## Checks run

| Check | Windows | Linux (WSL2 Ubuntu 24.04, ext4, non-root `dev`) |
| --- | --- | --- |
| `cargo test -p enouia-memory-host --lib owner_bootstrap` | 14 passed | 13 passed (the native Windows-evidence test is Windows-only) |
| `cargo clippy -p enouia-memory-host --all-targets -- -D warnings` | clean | |
| `cargo fmt --all -- --check` | clean | |

Linux CLI smoke with the real binary and the real Linux identity, in a
synthetic home `~/enouia-synthetic-bootstrap-smoke` inside the WSL
distribution. The home has the marker files and `storage_class: owner_data`,
and holds no real data. It was left in place because it carries the
do-not-delete marker.

1. `prepare` from `/tmp` with `storage_class: synthetic_test` was refused
   (`UnsafeHome`), as designed.
2. `prepare` in the home issued an operation and an `automation-segmented-owner-v1`
   descriptor.
3. With the first build, `commit` was refused as `Unauthenticated`. The journal
   had been written 0644 under the default umask, and the journal check
   correctly refused it. After the fix, the journal is kept 0600, and
   `status` and `commit` in new processes resolved the same Linux account.
4. `commit` recorded the creation with `empty_at_creation: true` and phase
   `recorded`. The metadata now reports a created Vault and its locator. The
   Vault root is 0700.

The unit suite first failed on Linux for the same reason: its fixture wrote
the marker 0644, which metadata publication refuses. The fixture now keeps the
marker owner-only, as an owner would.

## Not shown

A real VPS, a Vault created as a dedicated service account by systemd's user,
migration of the existing Windows Vault, and owner acceptance.
