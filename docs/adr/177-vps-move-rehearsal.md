# ADR-MEM-177 — Serving a moved Vault, and the VPS move runbook

Date: 2026-11-24. Status: Accepted (route: long-term primary on a VPS);
synthetic validation.

## Context

The long-term route puts the Morii Vault on a Linux VPS behind the remote
service. The move itself uses existing tools:

- ADR-MEM-136 export, verify, restore and activate;
- ADR-MEM-138, which shows that a Windows export restores on Linux.

No evidence covered the step after that, where the remote service runs on a
restored and activated root. The open questions were:

- whether a read grant confirmed before the move still authorizes;
- whether the drafted connector grant works there;
- whether the connector can open new windows, form memories and themes there;
- whether a restore that has not been activated is kept from serving.

The repository also had no written procedure for the move.

## Decision

- **Evidence.** The real-process daily-flow test (ADR-MEM-176) is reused on two
  roots: the original root and a moved root. The moved root goes through
  export, verify, restore into an empty directory, a check that the root cannot
  be opened before activation, then activation. On both roots the real binary
  runs the same steps:
  - draft and confirm the grant;
  - OAuth with the `approve` process;
  - search;
  - conversation and theme submission and replay;
  - reopen check.

  The test runs on Windows and Linux. Together with ADR-MEM-138's cross-OS
  restore, this covers export on the PC, restore on Linux and serve there.
- **Runbook.** `docs/deploy/MOVE_VAULT_TO_VPS.md` (Chinese) lists the steps:
  1. stop all local use;
  2. export and verify;
  3. transfer and verify again;
  4. prepare the Linux data region;
  5. restore and activate as the service account;
  6. point the service config at the restored root, keeping `host_id` and
     `read_grant_id`;
  7. go live and re-approve ChatGPT.

  It states the single-primary rule. After the export the PC copy is never
  written again. Runtime reading the VPS primary belongs to the later
  local-replica stage, which needs its own design.
- **Roadmap status.** IMPLEMENTATION_PLAN §15 records that the time-derived
  fade and archive events and the review suggestions were delivered by
  ADR-MEM-164. Semantic neighbors remain, deferred by Morii on 2026-11-24.

## Consequences

- The VPS move needs no new code. It is an operational procedure on top of
  ADR-136 and ADR-176, with evidence on both platforms.
- OAuth state is not in the export, so ChatGPT re-approves after the move.
  The Vault read grant moves with the Vault.
- No production API, schema, IPC or Runtime surface change.

## Verification

`remote_daily_process` has two tests, original root and moved root. Both pass on
Windows and on Linux (WSL2 Ubuntu 24.04).
