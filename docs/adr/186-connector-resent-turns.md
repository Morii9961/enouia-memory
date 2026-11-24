# ADR-MEM-186 — Refuse resent ChatGPT turns, with the reason

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

ChatGPT may resend turns it already submitted, for example by sending a whole
thread again at the end of the day under a new `request_id`.

- **Same window.** The capture's event locator collides, so the Vault refused
  the request, with no reason ChatGPT could act on.
- **New window.** After ADR-MEM-185, the turns would land in a new segment and
  be stored a second time.

## Decision

- **Check.** Before capture, a new request (one with no recorded capture for its
  `request_id`) is refused as invalid when any of its message IDs is already a
  captured turn of the same conversation. The conversation counts as its
  original or any `#pol_…` segment, under the connector's platform and account
  scope. `ConnectorWriter::resent_turns` returns those message IDs.
- **Reasons.** `ConnectorWriter::submission_problems` extends ADR-MEM-181's list
  with "turn m was already submitted in this conversation; send only new turns
  (resubmitting the same request_id replays)". The remote tools use it when a
  submission is refused.
- **Replay.** A resubmission of the same `request_id` is a replay. It skips this
  check and replays as before.

## Consequences

- No duplicate turns in either window, and ChatGPT is told exactly which turns
  to leave out.
- No schema, IPC or wire change.

## Verification

In `connector_formation::a_conversation_continues_across_windows`:

- A new request that resends `m1` after the window rotation is refused.
- Its problems name turn `m1`.
- The replay of the continued request reports no problems.
- The conversation records are unchanged.
