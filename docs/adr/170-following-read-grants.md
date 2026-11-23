# ADR-MEM-170 — Read grants that follow new windows and sources

Date: 2026-11-23. Status: Accepted (Morii's decision 2026-11-23, relayed by the
Runtime coordination session); synthetic validation.

## Context

ADR-MEM-168 made a grant's coverage gap visible and gave Morii a ready-made
refresh. He still had to confirm each refresh. Morii decided that grants of
registered Agents (Claude Code, Codex) should follow new windows and sources
automatically. The Private ceiling and the 90-day expiry stay, and he can
revoke at any time.

## Decision

`OwnerReadGrant` gains optional `follow_new_sources`. When it is false or
absent, the grant's bytes, hash and meaning are unchanged. When it is true, at
every read and request the grant also covers every policy revision of its own
subject and project, together with their captured, undeleted sources. This
includes windows and sources added after the grant. Everything else stays as it
was:

- **Unchanged checks.** The grant must be active, inside its validity, not
  revoked, and the exact current revision. The permit's sensitivity ceiling
  (at most the grant's) still caps what is read. A deleted source or memory
  leaves the coverage at once.
- **Permits.** A grant-backed permit computes the follow set when issued and
  recomputes it at every renewal. Renewal runs at each connection check: before
  and after every call, and every quarter second while idle (ADR-MEM-163/166).
  A live Agent connection therefore sees new memories without reconnecting. A
  permit may name followed policies and sources in its scope. Reads use the
  extended policy basis. Per-memory checks, source checks and Agent request
  drafts accept followed sources under any policy of the subject and project.
- **Requests.** Admission of Agent notes, correction reports and forget
  requests at commit time accepts a following, request-capable grant for any
  policy of its subject and project. This is evaluated on that commit's state,
  so replay is deterministic.
- **Coverage.** ADR-MEM-168 coverage reports no gap for a following grant and
  offers no refresh.

Turning following on or off is an ordinary grant revision: plan, confirm, commit.
It is Morii's explicit choice per grant. Nothing turns it on by default.

## Consequences

- Runtime registers or re-registers Agent grants with `follow_new_sources: true`
  and stops prompting for refreshes for them. The ChatGPT connector grant can
  use the same flag when it is deployed.
- Older binaries refuse a grant that carries the flag (closed fields), as with
  `allow_requests`. Move the pin before writing one.
- No IPC command or wire change. The grant schema adds one optional boolean.

## Verification

A synthetic Owner root is used, with a following grant (requests allowed) and a
fixed grant for another Agent, both listing the original window.

- **Reads.** After a later window gets a Runtime-captured turn and a formed
  memory, the following connection sees the new memory after its next renewal.
  The fixed one does not.
- **New permits.** A new permit may name the later source only under the
  following grant.
- **Coverage.** The following grant shows no gap; the fixed one does.
- **Requests.** A note on the later turn is saved under the following grant and
  refused under the fixed one.
- **Revocation.** Revoking the following grant ends the live connection at its
  next check.
- **Regression.** The existing grant, permit, request and coverage tests pass.
