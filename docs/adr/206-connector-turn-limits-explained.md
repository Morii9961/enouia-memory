# ADR-MEM-206 — Turn limits tell ChatGPT how to split

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

A connector submission is an ordinary Runtime capture request
(ADR-MEM-157). It holds 1 to 8 turns, and each turn's text is at most
16384 bytes of UTF-8 (`RUNTIME_CAPTURE_NATIVE_V1`). Both limits are part of
the shared capture contract and of the Vault's source bound, so the connector
does not change them.

A day's chat often has more than 8 messages. A long Chinese answer can pass
16384 bytes, which is about 5,000 Chinese characters. Until now, ChatGPT
learned the first limit only from `maxItems` and the second not at all. The
refusals said only "must hold 1 to 8 turns" and "at most 16 KiB", without
saying how to proceed.

## Decision

The limits stay. What ChatGPT is told changes:

- **Schema.** `turns` says to send a longer chat as several calls in order,
  each with its own `request_id` and the same `conversation_id`. A turn's
  `text` states the byte limit and says to split a longer message verbatim
  into consecutive turns of the same speaker.
- **Too many turns.** The refusal gives the count and the same splitting
  rule. It adds that each memory goes in the call that holds its quoted turn.
- **Turn too long.** The refusal gives the turn's size in bytes and suggests
  IDs such as `<id>-1` and `<id>-2`. An empty turn has its own reason.

Splitting loses nothing:

- The capture contract has no rule against consecutive turns of the same
  speaker, so a message split verbatim keeps its full text.
- Later calls continue the same chat (ADR-MEM-192).

## Verification

`remote_daily_process`, through the real remote process:

- **Nine turns.** The refusal reports "holds 9 turns; send 1 to 8 per call".
- **Long message.** A 21,000-byte user message is refused with its size and
  the IDs `m1-1` and `m1-2`.
- **Split.** The same message sent as `m1-1` and `m1-2` passes preflight.

The `connector_formation` and `remote_connector` suites pass unchanged.
