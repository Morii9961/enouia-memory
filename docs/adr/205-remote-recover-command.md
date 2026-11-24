# ADR-MEM-205 — `recover` finishes a publication an unknown outcome left half done

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

When a write's outcome is unknown, the remote service stops instead of
retrying (ADR-MEM-133, 187). The deployment guides then said "recover the
Vault before serving again", but `enouia-memory-remote` had no command for it.
If the interrupted write had already moved the Vault's current pointer, every
later command, `serve` included, failed with `RecoveryRequired`, and the owner
on the VPS had no supported way out short of restoring a backup.

The Vault already has an explicit forward recovery
(`SegmentedAutomationVault::recover`). It finishes a publication whose
segment and intent are on disk and whose pointers stopped between the old and
new head. It refuses anything else and never repairs on an ordinary open.

## Decision

`enouia-memory-remote recover <config> --confirm-recover`:

- **Opens normally.** If the Vault opens, nothing is written. The output is
  `{"recovered": false, "commit_id": <head>}`.
- **Rolls forward.** On `RecoveryRequired`, it reads the Vault descriptor's
  System actor and runs the Vault's forward recovery under it. The output is
  `{"recovered": true, "commit_id": <new head>}`.
- **Refuses.** Without the flag it changes nothing. On `Busy` it says which
  processes can hold the Vault. Any other failure tells the owner to restore
  the latest verified export.

Other commands that meet `RecoveryRequired` now name this command, and so
does the error `serve` exits with after an unknown outcome.

After `recover`, the client resends the same `request_id`. A write that
landed replays its receipt; one that did not is written then. Nothing is
retried automatically.

## Verification

`remote_read_admin_cli::recover_rolls_a_half_done_publication_forward_once`:

- **Stuck Vault.** A correction report fails after the current pointer moves.
  `read-status` then fails and names `--confirm-recover`.
- **Flag.** `recover` without the flag is refused.
- **Roll forward.** With the flag, it reports `recovered: true` and a new
  head, and `read-status` shows that head.
- **Second run.** The Vault opens cleanly, and `recover` reports
  `recovered: false` with the same head.
