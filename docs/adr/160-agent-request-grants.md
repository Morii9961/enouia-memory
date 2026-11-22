# ADR-MEM-160 — Persistent Agent registration through request-capable read grants

Date: 2026-11-22. Status: Accepted (Morii's R4.3 item D defaults); synthetic validation.

## Context

Morii's defaults for item D:

- register Claude Code and Codex;
- read up to Private, expiring after 90 days;
- writes limited to Agent notes, correction requests and forget requests;
- Morii confirms on the desktop (item D2);
- registration, renewal and revocation live in Runtime Settings.

Reads can already outlive formation windows through an Owner read grant
(ADR-MEM-141). The three request writers could not: their permits were built on
the current-policy reader, and note, correction and forget intake each required
the source's formation policy to be active and inside its window at commit
time. Every local Agent request would be refused after a window ended.

## Decision

- **Contract.** `OwnerReadGrant` gains optional `allow_requests` (default false,
  omitted from bytes when false, so existing grants keep their bytes and
  hashes). A request-capable grant must also have `source_read`. The schema adds
  the boolean.
- **Permits.** Add `authorize_granted_agent_note / _correction / _forget(owner,
  principal, grant_id, scope)`. Each requires the selected grant to be
  request-capable and to belong to `principal`, then builds the existing
  grant-backed read permit (`authorize_owner_grant_read`). The capability hash
  and every prepare/submit step are unchanged. Host:
  `QualifiedAgentHost::{note_writer, correction_reporter,
  forget_requester}_with_owner_grant`.
- **Intake.** A note, correction report or forget request is admissible when its
  policy is active and in its window, as before, **or** when the principal holds
  at commit time a grant that:
  - is active and request-capable;
  - is inside its validity;
  - covers the exact source, the exact policy revision and the source's
    sensitivity.

  Grants are commits on the same chain, so replay sees exactly the grants that
  existed at each request's commit; no record field changes. Everything else in
  intake stays as before:
  - quotes must resolve;
  - sources must be captured user statements;
  - the per-target and per-source caps apply;
  - deletion barriers and replay keys hold.

A registration is therefore one request-capable grant per Agent, managed with
the existing grant plan / set / renew / revoke workflow (Runtime Settings can
call the same native APIs). Renewal changes only the period; revocation sets
`active=false` and ends both reads and requests immediately.

## Consequences

- Older binaries refuse a grant carrying `allow_requests`, and refuse requests
  admitted only through such a grant. Move the pin before writing one.
- Like any grant, the scope lists explicit sources and policy revisions.
  Sources captured later need a grant update before Agents can annotate them.
- D2 (Morii's desktop queue for correction and forget requests) is separate.
  Agent correction reports are still consumed automatically by the captured
  worker (ADR-MEM-122) until D2 changes that.
- No IPC or wire change. The Workspace stays at 61 commands.

## Verification

Synthetic Owner root with a captured user statement and a target memory, with
the clock past the fixture formation window:

- Refused: the current-policy forget requester, a grant without
  `allow_requests`, and a request-capable grant without source reads (at plan
  time).
- A request-capable grant saves the forget request.
- After reopen (full replay), the same original request replays to the same
  receipt.
- Notes and correction reports share the same intake and permit path. Existing
  note, correction, forget and supersede tests, both native and over the pipe,
  pass unchanged.
