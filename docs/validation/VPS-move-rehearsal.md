# Serving a moved Vault: synthetic validation (2026-11-24)

Scope: [ADR-MEM-177](../adr/177-vps-move-rehearsal.md). The checks use a
synthetic segmented Owner Vault and the real `enouia-memory-remote` process on
loopback. No real Vault, VPS, tunnel or ChatGPT client was involved.

## Checks run

| Check | Windows | Linux (WSL2 Ubuntu 24.04) |
| --- | --- | --- |
| `remote_daily_process`: original root | passed | passed |
| `remote_daily_process`: exported, restored and activated root | passed | passed |
| `cargo clippy -p enouia-memory-host --all-targets -D warnings` | clean | |
| `cargo fmt --all -- --check` | clean | |

## What the moved-root test shows

1. The export verifies, and its head equals the export receipt's head.
2. The restore lands in an empty directory, and opening it before activation is
   refused.
3. After activation, the real process runs the full daily flow on the restored
   root:
   - drafts and confirms the connector grant;
   - completes OAuth with the `approve` process;
   - searches;
   - submits two conversations and a theme, each with preflight and replay;
   - leaves exactly three new memories, which the reopen check finds.

Together with the [cross-OS restore rehearsal](Owner-cross-platform-rehearsal.md),
this covers exporting on Windows, restoring on Linux and serving there.

## Not shown

These were not checked:

- a real VPS;
- copying a real export over the network;
- the real Vault;
- ChatGPT's own client;
- Morii's acceptance.
