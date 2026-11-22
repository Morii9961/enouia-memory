# ADR-MEM-159 — A read ceiling above a policy's maximum clamps per policy

Date: 2026-11-22. Status: Accepted; synthetic validation.

## Context

Morii decided that the ChatGPT connector may read everything up to
HighlySensitive (recorded with ADR-MEM-158; executed when the connector is
deployed). Two segmented checks refused any read scope or Owner read grant
whose `max_sensitivity` exceeded the maximum of **any** selected policy:

- `authorize_agent_read` / `authorize_owner_grant_read` (segmented Agent
  permits);
- `check_read_grant` (ADR-MEM-141 grant admission).

A real library has policies with different maxima, for example Private and
HighlySensitive formation batches. So one scope or grant could cover all of it
only at the lowest maximum, which hides every HighlySensitive memory.

## Decision

Remove those two refusals. Every read already clamps per policy: sources are
admitted at `min(ceiling, policy max)`, and Agent memory checks use
`scope.max_sensitivity.min(p.max_sensitivity)`. A memory's sensitivity never
exceeds its policy's maximum. A ceiling above a policy's maximum therefore
reads exactly what that policy allows, and one scope or grant can span policies
of different maxima.

Unchanged:

- a scope still cannot exceed its grant's ceiling;
- subject, project, policy existence, source coverage, deletion and expiry
  checks stay;
- the legacy (non-segmented) reader and Runtime capture scopes keep their
  checks.

## Consequences

- Older binaries refuse a grant commit whose ceiling exceeds a selected
  policy's maximum. Adopt this revision before writing such a grant. Grants and
  permits within the old rule behave as before, and replay of existing grant
  commits is unaffected.
- No persisted format, IPC or wire change.

## Verification

- A HighlySensitive Agent scope over Private policies is accepted and reads the
  same 8 memories a Private scope reads.
- Four tests encoded the old refusal:
  - in vault `segmented_agent`, host `connector_policy_semantics` and
    `initialized_agent_native`, the case now probes an unknown policy, which
    still refuses;
  - host `qualified_agent` now expects a Private ceiling over the correction's
    Public policy to be accepted, clamping to Public without borrowing the
    original policy's limit.
- All vault tests pass, as do the host Agent, grant, remote-service and
  read-admin tests.
