# ADR-MEM-192 — A later call of the same chat continues after its recorded turns

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

ChatGPT's short form (ADR-MEM-178) numbers the turns of every call from 1
and gives the first turn no parent. The Vault refuses a turn whose sequence
the conversation already holds. As a result, the second call of a chat under
the same `conversation_id` was refused with `Denied`, and `problems` was
empty.

That happens every time a chat is submitted in more than one call: across
days, or over more than eight turns. ADR-MEM-191 also asks ChatGPT to reuse
one `conversation_id` per chat, so this would have been hit on the first real
multi-day chat.

## Decision

`ConnectorWriter` already decides the target conversation (the original or
its window segment, ADR-MEM-185). That step now also places the new request's
turns.

- **New request.** If the lowest sequence of the request is at or below the
  highest recorded sequence of the target conversation, the request is
  treated as a continuation:
  - all its sequences shift so the lowest becomes one past the last recorded
    turn;
  - its first turn, if it has no parent, follows the last recorded turn.

  Requests whose sequences already follow the record are unchanged.
- **Replay.** Each turn takes the sequence and parent recorded for its
  message ID. The capture's replay check compares turn digests, and those
  include sequence and parent. A replay therefore matches its record whatever
  was captured after it.
- Preflight goes through the same step, so a dry run reports what the real
  call would do.

## Verification

- `connector_formation::a_later_call_of_the_same_chat_continues_after_its_recorded_turns`:
  - Three short-form calls of one chat, each numbered from 1 with no parent,
    are captured and formed.
  - The preflight of the second call writes nothing.
  - Replays of all three calls replay.
  - The recorded turns are sequences 1, 2, 3, each the child of the one
    before.
- A probe before the change showed the second call refused with
  `Vault(Denied)`.
- `remote_daily_process` (real binary, Windows and Linux) submits the next
  day's call of an existing chat in the short form. It is captured and
  replays.
- `a_conversation_continues_across_windows` (ADR-MEM-185/187) passes
  unchanged.
