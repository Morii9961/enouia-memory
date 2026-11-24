# ADR-MEM-185 — ChatGPT conversations continue across capture windows

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The connector captures each submission under a capture window: a policy with
at most 128 fresh source IDs that lasts `window_days`. When the current window
has ended or filled, the next submission opens a new one.

A capture permit covers one window, and appending to an existing conversation
requires that conversation's earlier turns to be inside that window. ChatGPT
threads often run for days. Once a thread's later turns were submitted after a
window rotation, every continuation was refused with `Denied`, so those turns
and their memories were lost.

## Decision

Before capturing, `ConnectorWriter` checks the recorded conversations of its
platform and account scope. If the submitted `conversation_id` already has
turns outside the chosen window, the submission continues the conversation
there as a segment:

- **Segment ID.** The conversation ID becomes `<conversation_id>#<window policy
  ID>`, with the original truncated so the result fits in 256 characters.
- **Parents.** A parent message outside both this submission and the segment's
  recorded turns is dropped; parents inside them are kept.
- **Analysis.** The submitted statements are keyed to the segment, so formation
  is unchanged.

The rule depends only on recorded state and the chosen window. A replay
(ADR-MEM-158) finds the same window and rebuilds the same request, so request
hashes still match. Conversations confined to one window are untouched.

## Consequences

- Long ChatGPT threads keep being captured. In Runtime a thread that spans
  windows appears as one conversation per window. The later ones carry the
  `#pol_…` suffix and start without a cross-window parent.
- **Resolved limit.** Resubmitting a request whose window had already ended was
  refused (`Denied`). [ADR-MEM-187](187-connector-late-replay.md) answers such
  replays from the record instead.
- No schema, IPC or wire change.

## Verification

`connector_formation::a_conversation_continues_across_windows`, with a one-day
window:

1. A turn is captured, then the clock moves two days.
2. The next turn of the same conversation is captured in a new window and forms
   a memory.
3. A third turn joins the same segment with its parent kept.
4. The continued request replays. The first request's replay is refused once
   its window has ended.
5. The recorded conversations are the original, with 1 turn, and the segment,
   with 2 turns of which one has a parent.
