# ADR-MEM-193 — Structural capture refusals come with a reason

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The capture refuses some submissions with `AutomationFault::Conflict`:

- a `request_id` replayed with different content, such as an extra turn or
  edited text;
- a new request whose turn names a `parent_message_id` that is not a turn of
  the conversation.

ADR-MEM-181 explains `Invalid` refusals, but `Conflict` fell through to the
generic arm. ChatGPT received only `{"error": "Vault(Conflict)"}`. Probing
the first case showed exactly that, with empty `problems`. ChatGPT reuses its
own request keys easily, for example by updating the day's submission, so
this would be met in daily use.

## Decision

- `ConnectorWriter::submission_problems` adds two reasons:
  - **Changed replay.** The `request_id` is recorded, and some turn of the
    submission is not recorded in that conversation with the same text. The
    reason says to resubmit unchanged to replay, or to use a new
    `request_id` for new turns. An exact replay still reports nothing.
  - **Unknown parent.** The request is new, and a turn's `parent_message_id`
    is neither an earlier turn of the request nor a recorded turn of the
    conversation (original or segment). The reason says to leave
    `parent_message_id` out. ADR-MEM-192 then continues the conversation
    after its last turn.
- The connector treats a capture's `Conflict` like `Invalid`. It answers
  `invalid_submission` with `problems` when any are found. With none, it
  keeps the previous generic error.
- The turn messages of ADR-MEM-186 and ADR-MEM-191 now come from one read
  of the conversation's recorded message texts.

## Verification

- `connector_formation::changed_replays_and_unknown_parents_are_explained`:
  - An exact replay reports no problems.
  - The same `request_id` with an extra turn is refused with `Conflict`
    and exactly one reason, the changed replay.
  - A new request with parent `m9` is refused with exactly one reason, the
    unknown parent.
- `remote_daily_process` (real binary): resubmitting a short-form
  `request_id` with edited text returns `isError` with the changed-replay
  reason.
- The ADR-MEM-185/186/191/192 tests pass unchanged.
