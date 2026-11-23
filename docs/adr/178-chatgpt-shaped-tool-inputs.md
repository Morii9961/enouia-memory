# ADR-MEM-178 — ChatGPT-shaped tool inputs on the remote service

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The remote service exposed the Agent read tools with their frozen envelopes.
Every call had to construct:

- `schema_version` and `kind`;
- `capability` and `operation`;
- a `request_id` matching `req_` plus a version-4 UUID;
- every required argument. Search needs `filter`, `cursor`, `limit`,
  `max_bytes` and `include_facets`. Recall needs `at` and `event_window`.

The advertised input schemas were a top-level `$ref` into bundled `$defs`, and
ChatGPT's connector client handles `$ref` only partly.

Recall bounds must also stay within the service's attention profile, which a
client cannot know.

The write tools had the same burden:

- a capture request with its constants;
- twelve fields for every turn;
- two six-field time objects for every statement;
- a strict request ID.

An honest model following these schemas would still often produce a call that
Memory refuses.

## Decision

A thin layer in the remote `ConnectorSession` (`mcp::remote::chatgpt`). Memory
still validates every call with the same typed parsing as before.

- **Read inputs.** `tools/list` replaces the input schema of `memory_search`,
  `memory_read`, `memory_source`, `memory_related`, `memory_recall` and
  `session_start` with a flat schema that has no `$ref`:

  | Tool | Inputs |
  | --- | --- |
  | `memory_search` | `query`, optional `form`, `attention`, `adoption`, `limit`, `cursor` |
  | `memory_recall` | `text`, optional `intent`, `conditions`, `limit` |
  | `memory_read` | `memory_id`, optional `include_source_text` |
  | `memory_source` | `source_id`, optional `revision`, `from_byte` |
  | `memory_related` | `memory_id` |
  | `session_start` | optional `topic` |

  The `form`, `attention` and `adoption` enums are taken from the catalog
  schema.

  On a call, the layer adds the envelope:
  - a fresh request ID and the evaluation time;
  - the catalog filter defaults;
  - search sizes of 20 items and 64 KiB;
  - recall sizes defaulting to, and capped at, the profile's `max_items` and
    `max_bytes`;
  - fixed byte limits for read, source and related.

  Output schemas are unchanged.
- **Write inputs.** `memory_submit_conversation` accepts a short request with
  `request_id`, `conversation_id` and `turns`. Each turn needs only
  `message_id`, `speaker` and `text`, and may add `occurred_at`. The layer
  fills in:
  - the request constants and a null title;
  - a linear `main` branch in the given order, with sequence and parent;
  - unknown original time and zone, language `und` and no attachments;
  - unknown `event_time` and `valid_time` and a null `note` on each statement.

  `interpretation` is advertised as its enum, and `kind` as the forms this
  service allows.
- **Request keys.** For both write tools, a `request_id` that is not already a
  request ID (1–128 characters) maps to one request ID through SHA-256 under
  `chatgpt-request-key-1`. The same key therefore replays.
- **Full forms pass through.** A read call carrying `schema_version`, and a
  write call whose fields are already present, are left unchanged. Only absent
  fields are filled in. Unknown fields in a short read call are refused with
  `invalid_arguments`.

## Consequences

- ChatGPT can call every tool from what `tools/list` advertises, and the custom
  instructions can stay short.
- Local Agents (Claude Code, Codex) keep the full protocols. This layer exists
  only on the remote service.
- The `ConnectorSession::new` signature gains the ID source and the attention
  profile. There is no wire, IPC or schema change. The Runtime surface records
  the remote service files.

## Verification

`remote_daily_process` (real binary, original and moved roots):

- No advertised input schema contains `$ref`.
- A short search equals the full-envelope search.
- A short `session_start` answers.
- A short search with an unknown field is refused with `invalid_arguments`.
- A short conversation forms a memory from:
  - a free-form key;
  - an assistant turn and a user turn of three fields each;
  - a statement without times.

  The same key replays.
- A theme with a free-form key forms and replays.
- Short `memory_read`, `memory_recall` (with a profile-capped size) and
  `memory_related` calls answer.

`remote_connector` and `remote_service` pass unchanged apart from the
constructor.
