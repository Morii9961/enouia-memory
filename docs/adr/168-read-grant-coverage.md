# ADR-MEM-168 — Owner read grant coverage and refresh plans

Date: 2026-11-23. Status: Accepted (daily-use follow-up to ADR-MEM-141/160); synthetic validation.

## Context

An Owner read grant lists explicit policy revisions and sources (ADR-MEM-141).
ADR-MEM-160 noted the consequence: sources captured later need a grant update
before Agents can read or annotate them.

Daily use runs into this. The ChatGPT connector opens or reuses capture windows
every day (ADR-MEM-156/158), and each window is a policy, so ChatGPT cannot read
the memories it formed yesterday until Morii updates its grant. A local Agent
registered in Runtime Settings has the same gap.

Morii had to build the next revision by hand from `owner_read_status`. Nothing
told Runtime or him that a grant had fallen behind.

## Decision

`SegmentedAutomationVault::owner_read_grant_coverage(owner, grant_id)` is a
native, Owner-only read that writes and authorizes nothing. It returns:

- **`grant`**: the current grant;
- **`uncovered_policies`**: current revisions of policies in the grant's own
  subject and project that it does not list, including later capture windows;
- **`uncovered_sources`**: captured, undeleted sources of the listed and uncovered
  policies that it does not list (unused window IDs are not sources);
- **`refreshed`**: `revision + 1` listing all of them, otherwise identical. Its
  principal, ceiling, `source_read`, `allow_requests` and validity are unchanged.
  It must already pass the plan check. It is absent when nothing is uncovered,
  when the grant is revoked, or when it would exceed the grant limits.

The grant does not change. Applying a refresh is the ordinary
`plan_owner_read_grant` / `commit_owner_read_grant` flow with Morii's
confirmation. `enouia-memory-remote read-refresh-plan <config> <grant-id>`
reports the gap on stderr and prints the plan for `read-set`.

Automatically following future windows, so that a grant covers sources captured
after it, would change what a grant authorizes. That stays a separate decision
for Morii (recorded in the handoff queue). This ADR keeps every widening
explicit.

## Consequences

- Runtime Settings can show, for each registered Agent and the connector, how
  many windows and sources its grant misses. It can offer "include new
  sources", showing the plan before Morii confirms.
- No persisted format, IPC command or wire change. The grant shape is unchanged.

## Verification

A synthetic Owner root is used.

- A fresh grant has no gap and no refresh. A non-Owner caller is refused.
- A later window of the same subject, plus a later capture in the old policy,
  are reported as one uncovered policy and two uncovered sources, and the read
  writes nothing. The refresh is revision 2 with both policies and all four
  sources and otherwise identical. The stored grant is unchanged until the
  refresh is planned and committed, after which there is no gap.
- A revoked grant still reports a new gap but offers no refresh.
