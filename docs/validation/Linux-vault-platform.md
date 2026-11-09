# Linux Vault platform — validation (2026-11-09)

Scope: [ADR-MEM-131](../adr/131-linux-vault-platform.md), branch
`claude/vault-linux-platform` from main `96ea547`. Linux runs used a WSL2
Ubuntu 24.04 distribution set up for this purpose: a non-root `dev` user, the
repository cloned onto the distribution's own ext4 filesystem, gcc 13.3, and
the pinned Rust 1.98.1. Dependencies were fetched with `--locked`.

## Linux results

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-vault --locked --no-fail-fast` | 20 test binaries passed, including the 3 `platform::linux` unit tests |
| `cargo clippy --workspace --all-targets --locked --exclude enouia-memory-workspace-app -- -D warnings` | clean |
| `cargo fmt --all -- --check` | clean |
| `cargo test --workspace --locked --no-fail-fast --exclude enouia-memory-workspace-app` | 124 test binaries passed; one Workspace lib test fails only at the default 32 test threads (see below) |

The Tauri reference shell (`enouia-memory-workspace-app`) is excluded on
Linux. Its build needs desktop libraries such as dbus and webkit, and a VPS
does not run it.

Running on Linux found and fixed the following:

- The new unit test expected one broad grant for a 0644 file. Group read and
  other read are two.
- `backup.rs` asserted Windows ACL inheritance. Linux now asserts that a 0755
  root is not owner-only, and that once restricted, the 0700 root protects
  children whose modes follow the umask.
- `root.rs` used `mklink /J`, NTFS/ReFS names and UNC paths. Linux now uses a
  symlink as the reparse point, local filesystem names, and no UNC cases. A
  write through the link is still refused and nothing lands outside.
- The Vault dependency boundary test now allows the pinned Linux-only `libc`.

Workspace timing: `full_session_ask_replays_a_concurrent_input_commit` (and
once `..._an_input_published_while_the_call_is_paused`) waits at most about one
second for synthetic index/writer contention to clear. At 32 parallel test
threads that is too short. With 1, 4 or 8 threads the whole Workspace lib suite
passes, and the test passes 5/5 alone. On main the same test fails 5/5 on Linux
even alone, because the old fallback could not inspect ACLs. The retry budget
is unchanged in this slice.

## Windows results

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-vault --locked` | 20 test binaries passed |
| `cargo test -p enouia-memory-contract --locked --test boundaries --test runtime_surface` | passed |
| `cargo clippy` for the Vault (Windows and Linux targets) and `cargo fmt --all -- --check` | clean |
| `python tools/integration/runtime_surface.py` | aggregate `49ed1b98…0797` logged |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` (Windows) | clean |
| `cargo test --workspace --locked` (Windows) | not completed: drive E: ran out of space while linking; to be rerun after disk cleanup |

The Cargo.lock change is one line: `libc` in the Vault's dependency list.

## Not shown

POSIX ACLs on a filesystem with ACLs enabled, remote and FUSE mounts, a real
Vault entry on Linux, and the VPS itself.
