# ADR-MEM-188 — Nightly backup of the remote service's Vault

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use, MV-9.4
independent recovery); synthetic validation.

## Context

Once the Vault lives on a VPS (ADR-MEM-177), ChatGPT writes to it every day,
but nothing backs it up. `enouia-memory-owner-backup export` (ADR-MEM-136)
needs a stopped root and refuses while any process holds the Host lease, and
the service runs continuously. Backups therefore depended on Morii remembering
to stop the service, export, verify and start it again.

## Decision

- **`enouia-memory-remote backup <config> <backup-dir>`.** It reads
  `vault_root` from the service config and creates a new directory
  `<backup-dir>/<YYYYMMDDTHHMMSSZ>` (UTC). Then it exports the segmented Owner
  root into it with the ADR-MEM-136 export, verifies the export independently
  and prints `{export, commit_id, sequence, files, verified}`.
  - A refused export (`Busy` while the service runs) removes the directory
    only if it is still empty.
  - The command never stops a process. It ships in the same binary as the
    service, so one deployment carries both.
- **Nightly script and units** (`docs/deploy/linux/`):
  - `enouia-memory-nightly-backup` runs as root. If `enouia-memory-remote`
    is active, it stops the service, runs `backup` as the `enouia` account,
    then starts the service again whatever the backup's result. The script
    exits non-zero if the backup or the restart failed.
  - If the service was not active (for example, left stopped after an
    unknown outcome), the script backs up and does not start it.
  - `enouia-memory-backup.service` (oneshot) and `enouia-memory-backup.timer`
    run it daily at 04:30 Asia/Shanghai, with `Persistent=true`.
- **No automatic pruning.** Old exports are Morii's backup material. Pruning
  them and copying them off the VPS (the existing restic adapter) remain
  Morii's own steps.

Killing the service mid-request is the crash case the Vault's commit
protocol already covers. An unknown outcome is answered by request replay
(ADR-MEM-187). No signal handling is added.

## Verification

`remote_daily_process` runs on Windows and Linux against the real binary:

- While the service runs, `backup` fails with `Busy` and leaves no directory.
- After the service is killed, `backup` succeeds. Independent
  `verify_owner` of the printed directory reports the printed sequence.
- The Vault then reopens with the expected memories and queued requests.

The script's control flow was checked on Ubuntu 24.04 with stub `systemctl`
and `runuser`:

| Service state | Backup result | Script steps | Exit |
|---|---|---|---|
| Active | Succeeds | Stop, back up, start | 0 |
| Active | Fails | Stop, back up, start | 1 |
| Inactive | Succeeds | Back up only | 0 |

`systemd-analyze calendar` accepts the timer expression.
