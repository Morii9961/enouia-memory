# ADR-MEM-179 — Unapproved OAuth registrations lapse after an hour

Date: 2026-11-24. Status: Accepted (Morii approved after the failure mode was
explained); synthetic validation.

## Context

The remote service's OAuth authority allows at most 16 registered clients
(ADR-MEM-132). Any caller can register, and a registration keeps its slot even
if Morii never approves it with his one-time code.

On a public address such as a quick tunnel (ADR-MEM-176), repeated anonymous
registrations can take every slot. Morii then cannot add the ChatGPT connector
until he clears them one by one with `oauth-revoke`. Nothing leaks: approval
still needs Morii's code. The likelihood is low because quick-tunnel addresses
are random and short-lived.

## Decision

- **Pending clients.** The OAuth state gains an optional `pending` map from
  client ID to registration time.
  - A new registration enters it.
  - A successful approval at the authorize page removes it.
  - `oauth-revoke` removes it as well.
- **Lapse.** At registration and at the authorize page, any pending client
  registered at least `PENDING_CLIENT_TTL_MS` (one hour) ago is removed,
  together with its codes. Removing it frees the slot, and that client can no
  longer authorize, even with a valid approval code.
- **Unchanged.** Approved clients never lapse. Their tokens, rotation and
  revocation work as before.
- **Older state files.** A state file without `pending` loads unchanged, and the
  clients it already lists count as approved.

## Consequences

- Stray registrations clear themselves within an hour. A legitimate ChatGPT
  connector registers and is approved within minutes, well inside the window.
- An empty `pending` map is omitted from the state file. No wire, IPC or schema
  change.

## Verification

`remote_oauth::unapproved_registrations_lapse_and_free_their_slots`:

1. One approved client plus 15 never-approved registrations fill every slot.
2. A millisecond before the hour, registration is still refused.
3. At the hour, a lapsed client is refused at the authorize page even with a
   valid approval code.
4. A new registration then succeeds.
5. The approved client still refreshes its token.
6. The status lists the approved and the new client but none of the lapsed
   ones, and only the new one is pending.

The other OAuth and remote service tests pass unchanged.
