# Linux Vault platform — validation (2026-11-09)

Scope: [ADR-MEM-131](../adr/131-linux-vault-platform.md), branch
`claude/vault-linux-platform` from main `96ea547`. No Linux machine was
available. This PC has no WSL distribution and no Linux C compiler; only the
Rust std library for `x86_64-unknown-linux-gnu` was added to the pinned
toolchain.

## Checks run

| Check | Result |
| --- | --- |
| `cargo check -p enouia-memory-vault --all-targets --locked --target x86_64-unknown-linux-gnu` | compiles, including the Linux unit tests |
| `cargo clippy -p enouia-memory-vault --all-targets --locked --target x86_64-unknown-linux-gnu -- -D warnings` | clean |
| `cargo test -p enouia-memory-vault --locked` (Windows) | 20 test binaries, all passed |
| `cargo clippy -p enouia-memory-vault --all-targets --locked -- -D warnings` (Windows) | clean |
| `cargo fmt --all -- --check` | clean |
| `python tools/integration/runtime_surface.py` | aggregate `49ed1b98…0797` logged |
| `cargo test -p enouia-memory-contract --locked --test runtime_surface` | passed |

The Cargo.lock change is one line: `libc` in the Vault's dependency list.

## Not run

- The three Linux unit tests in `platform/linux.rs` (restriction of directories
  and files, group/other bits as broad grants, real free space and volume
  kind). They need a Linux host.
- `cargo check` of the host crate for Linux. It fails before Rust code, in the
  `libsqlite3-sys` and `ring` build scripts, because there is no
  `x86_64-linux-gnu-gcc`.
- POSIX ACL behaviour on a filesystem that has ACLs enabled, behaviour on
  remote and FUSE mounts, and the full Vault suite on Linux.

Running these needs a Linux environment: a WSL distribution on this PC, a Linux
CI runner, or the VPS itself.
