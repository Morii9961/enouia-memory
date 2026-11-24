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

## Rerun at `main@e856156` (after ADR-MEM-171 to 174)

The same environment and command, now also covering `enouia-memory-init`. All
four packages pass: 123 test binaries, no failures. The Linux-built host tests
now also include connector entities (ADR-MEM-171) and ChatGPT themes
(ADR-MEM-174). The entity identity and capsule tests are Windows-only, because
they drive the trusted owner gesture.

## Rerun at `main@458a571` (after ADR-MEM-176 to 186)

The same environment and per-package command for `enouia-memory-contract`,
`-vault`, `-init` and `-host`. All four pass: 124 test binaries, no failures and
no panics. The run includes the real-binary ChatGPT daily flow on original and
moved roots (`remote_daily_process`: ADR-MEM-176/177/178/181–184), the OAuth
lapse (ADR-MEM-179), and conversation segments and resent-turn refusal
(`connector_formation`: ADR-MEM-185/186).

`cargo test --workspace` is not a Linux check here. A workspace member's
`libdbus-sys` build script needs the system D-Bus development package, which
this WSL image does not install. The Linux service needs none of these
packages.

## Rerun at `main@729812d` (after ADR-MEM-187 to 193)

The same environment and per-package command for `enouia-memory-contract`,
`-vault`, `-init` and `-host`. All four pass: 124 test binaries, no failures
and no panics. The run adds coverage for the following:

- **Late replays** (ADR-MEM-187).
- **Nightly backup command** (ADR-MEM-188). It refuses while the real
  service runs and succeeds after the service is killed.
- **Content-free access log** (ADR-MEM-189).
- **Grant-end notice** (ADR-MEM-190).
- **Reused message IDs** (ADR-MEM-191).
- **Same-chat continuation** (ADR-MEM-192). This includes a next-day
  short-form call through the real binary.
- **Explained capture conflicts** (ADR-MEM-193).

## Rerun at `main@6026b78` (after ADR-MEM-194 to 203)

The same environment and per-package command. All four packages pass: 124
test binaries, no failures and no panics. The run adds coverage for the
following:

- **Busy hint** (ADR-MEM-194).
- **Turn-time conversion and argument reasons** (ADR-MEM-195/196).
- **OAuth scope and resource tolerance** (ADR-MEM-197).
- **MCP header tolerance** (ADR-MEM-198).
- **Session eviction and continuity across permit renewal** (ADR-MEM-199/200).
- **`Expect: 100-continue`** (ADR-MEM-201).
- **Not-found note** (ADR-MEM-202).
- **Per-member theme refusals** (ADR-MEM-203).

The full Windows `enouia-memory-host` suite also passed at the same point
(58 test binaries).
