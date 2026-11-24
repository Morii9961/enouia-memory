# ADR-MEM-194 — A plain Busy message and the VPS renewal steps

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

Only one process opens the Vault at a time. Every `enouia-memory-remote`
command that opens it while the service, Runtime, a backup or another
command holds it failed with `vault: Busy`, which says nothing about what to
do. Renewing the read grant after a `grant_notice` (ADR-MEM-190) needs
exactly that ordering on a VPS: stop, renew, start. The grant document listed
the commands but not the systemd sequence.

## Decision

- When the Vault is held, commands that open it now fail with: "vault: Busy;
  another process holds the Vault (the service, Runtime, a backup or another
  command); stop it first". `backup` already explained Busy (ADR-MEM-188).
  Other failures are reported as before.
- `CONNECTOR_READ_GRANTS.md` gains "在 VPS 上续期（systemd）". It gives these
  steps:
  1. Stop the service.
  2. Run `read-status`.
  3. Run `read-renew-plan` as `enouia` into a file.
  4. Review the plan, then run `read-set --confirm-owner-read`.
  5. Start the service and check the new end on the startup log line
     (ADR-MEM-189).

  It also suggests a manual backup run first.

## Verification

`remote_daily_process` runs `read-status` while the real service runs. The
command fails, and stderr names the held Vault.
