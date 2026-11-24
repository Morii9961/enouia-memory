# ADR-MEM-191 — A reused message ID is not a resend

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

ADR-MEM-186 refuses a new request whose turns this conversation already
holds. It matches turns by message ID alone and tells ChatGPT to "send only
new turns".

ChatGPT cannot see real message IDs, so it invents them, and the obvious
choice is `m1`, `m2`, … on every call. On the next day's submission of the
same chat, a genuinely new message called `m1` would be refused as already
submitted. Following the hint, ChatGPT would drop the message, and Morii's
words would be lost.

## Decision

- `ConnectorWriter::resent_turns` reports, for each already-recorded message
  ID, whether the recorded text equals the new turn's text. The capture
  stores turn text unchanged, so the comparison is exact.
- `submission_problems` words the two cases differently:
  - **Same text (a resend).** The ADR-MEM-186 reason as before: send only
    new turns.
  - **Different text.** "message_id … already names a different message in
    this conversation". ChatGPT is asked to give the new message an ID never
    used in this chat, for example the date and its position.
- Both cases still refuse the request before the Vault. Nothing is written
  under a duplicate ID.
- The submit tool's schema now describes both IDs:
  - `conversation_id`: one stable ID per chat, reused in every later
    submission of that chat;
  - `message_id`: unique within the chat across all submissions (for example
    `2026-11-24-3`).
- The custom-instructions paragraph says the same.

## Verification

`connector_formation::a_conversation_continues_across_windows` resubmits
message ID `m1` in a new request:

- **Original text.** Refused with the resend reason, as before.
- **New text.** Refused with exactly one problem, the reused-ID reason.

The real-process daily flow and the other remote suites pass unchanged.
