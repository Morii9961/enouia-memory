# ADR-MEM-196 — Short read calls say why they are refused; a large limit pages

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The short read tools (ADR-MEM-178) answered every failure to complete their
arguments with a bare `invalid_arguments`. That covered an unknown field such
as `top_k`, a wrong type, and `days` outside 1 to 366. Separately,
`memory_search` passed ChatGPT's `limit` through unchanged. The read session
allows 1 to 100 per page, so a limit of 200 came back as an opaque
`invalid_request`.

## Decision

- `chatgpt::read_arguments` now returns `Result<Value, String>`:
  - a parse failure gives the parser's message, which names the field;
  - `days` out of range gives "days must be 1 to 366";
  - a tool without a short form says so.
- The connector answers these failures with `invalid_arguments` and
  `problems: [reason]`, as ADR-MEM-195 does for the write tools.
- Search `limit` is clamped to 1 to 100. The reply's cursor pages through the
  rest, so asking for more never fails. Recall was already capped at the
  attention profile (ADR-MEM-178).

## Verification

`remote_daily_process` (real binary, Windows and Linux) checks three calls:

- `days: 0` is refused with "days must be 1 to 366".
- A search with `top_k` is refused, and the problem names `top_k`.
- A search with `limit: 500` succeeds.

`remote_connector` and `remote_http_opening` pass unchanged.
