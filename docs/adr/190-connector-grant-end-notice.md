# ADR-MEM-190 — Tool results warn before the read grant ends

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The connector reads under Morii's Owner read grant, which has a fixed end
(`valid_until`). When the grant ends, the service cannot obtain a read permit
and exits. ChatGPT then sees only gateway errors, and the startup log line
(ADR-MEM-189) is the only warning beforehand. Morii talks to ChatGPT every day
but rarely reads the server's journal.

## Decision

- `ConnectorSession::with_grant_end(end)` sets the grant's end. The service
  reads it once at startup from `owner_read_grant_until`. A renewal needs the
  service stopped anyway, because only one process opens the Vault.
- From `GRANT_NOTICE_MS` (14 days) before the end, every `tools/call` result
  whose `structuredContent` is an object gains a `grant_notice` field. The
  text content is rewritten to match. The field has three parts:
  - `ends_at`: the grant's end time.
  - `days_left`: whole days remaining, with no negative values.
  - `message`: asks ChatGPT to tell Morii to renew the grant with
    `enouia-memory-remote read-renew-plan`.
- `initialize`, `tools/list` and JSON-RPC errors are unchanged. Outside the
  14 days, results are unchanged.
- The ChatGPT custom-instructions paragraph asks it to relay `grant_notice`.
- Nothing about expiry itself changes: after the end the service still exits,
  and renewal stays Morii's own step.

## Verification

`remote_connector::tool_results_carry_a_grant_notice_near_the_grant_end`
checks three cases:

- **Grant ends in 3 days.** A write preflight result carries `grant_notice`
  with `days_left: 3` and a renew message, and its text content matches.
  `tools/list` carries no notice.
- **Grant ends in 30 days.** No notice.

The real-process `remote_daily_process` flow and the other remote suites pass
unchanged.

## Correction (same day)

The first implementation looked up `result.structuredContent` with mutable
JSON indexing. On a JSON-RPC error reply, that lookup inserted a `result`
member next to `error`, which produced invalid JSON-RPC. It happened only
during the notice period, and was found by probing the real binary. The
lookup now goes through `get_mut` and leaves error replies untouched. The
`tools/list` rewrite (ADR-MEM-178/180) had the same pattern and was fixed the
same way.

The test now also sends a `tools/call` that the read session answers with a
JSON-RPC error. The reply has no `result` and no notice.
