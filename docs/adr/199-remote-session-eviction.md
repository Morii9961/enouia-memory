# ADR-MEM-199 — A full MCP endpoint ends its least recently used session

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The remote endpoint holds at most `MAX_SESSIONS` (4) MCP sessions per
permit generation, and a generation lasts up to four minutes (ADR-MEM-133).
A fifth `initialize` was answered with 503 until the generation renewed.
ChatGPT may open a session per chat or per tool batch, and it rarely sends
`DELETE`. Opening a few chats within those minutes would then fail for no
reason Morii can act on.

## Decision

- Each session records when it was last used. Use means its creation or any
  request routed to it.
- When the endpoint is full, a new `initialize` ends the least recently used
  session, then opens the new one. The ended session's next request gets
  404. The MCP transport specification tells clients to initialize again on
  404, so the client recovers on its own.
- The limit itself, the per-generation reset and the unknown-outcome halt
  (503) are unchanged. Memory per process stays bounded.

## Verification

`remote_http::initialize_binds_one_session_to_the_bearer_subject_and_protocol_version`
opens four sessions, then uses session 0. A fifth `initialize` then
succeeds:

- the endpoint still holds four sessions;
- session 1, the least recently used, answers 404;
- the remaining close and `DELETE` checks pass as before.

The other remote suites pass unchanged.
