# Linux regression after ADR-MEM-161 to 170 (2026-11-23)

Scope: synthetic tests only, no real Vault, no deployment.

- Commit: `main@512ad0c`, which includes ADR-MEM-161 to 170.
- Environment: WSL2 Ubuntu 24.04 on the development machine, using the pinned
  toolchain rustc 1.98.1, with `RUST_TEST_THREADS=8` and a clean build cache.
- Command: `cargo test --locked --no-fail-fast` for `enouia-memory-contract`,
  `enouia-memory-vault` and `enouia-memory-host`.

## Result

All three packages pass: 110 test binaries, no failures and no panics.

The Linux-built host tests include the ones that the remote connector depends on:

- `owner_read_grant`: 9 tests, among them grant coverage and refresh (ADR-MEM-168)
  and following grants (ADR-MEM-170);
- `remote_service`;
- `agent_forget`.

Windows-only tests (named pipes, process enrollment, the stdio bridge) are
compiled out on Linux, as designed.

## Not covered

- No real Vault, OAuth deployment, TLS, reverse proxy or ChatGPT interop.
- No Linux persistent-filesystem durability beyond what the synthetic tests
  exercise.
- The Workspace Tauri application is not built on Linux.
