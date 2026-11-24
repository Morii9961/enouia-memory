# ADR-MEM-198 — Missing version and Accept headers follow the specifications

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The remote MCP endpoint (ADR-MEM-130) refused two requests that the
specifications allow.

- **No `MCP-Protocol-Version` header after `initialize`.** The endpoint
  answered 400. The MCP transport specification lets a server rely on the
  version negotiated at initialization when the header is absent. This
  server negotiates exactly one version per session.
- **No `Accept` header.** The endpoint answered 406. RFC 9110 says a request
  without `Accept` accepts any media type.

Either refusal would end a ChatGPT connection whose client omits the header,
and we cannot test that against ChatGPT before the live hookup.

## Decision

- **Missing version header.** The session's negotiated version
  (`2025-11-25`) applies. A header naming any other version is still 400.
- **Missing `Accept`.** The request is treated as accepting JSON. An
  `Accept` header that excludes JSON and `*/*` is still 406.
- Everything else stays as it was:
  - `Content-Type` is still required;
  - a session ID is required after `initialize`;
  - origin, bearer, size and batch checks are unchanged;
  - `GET` still answers 405 with `Allow: POST, DELETE`.

## Verification

`remote_http::initialize_binds_one_session_to_the_bearer_subject_and_protocol_version`
covers the four cases:

- A request without the version header is served.
- `2025-03-26` is still refused.
- `Accept: text/event-stream` is still 406.
- A request without `Accept` is served.

The session records both extra `tools/list` calls. The other remote suites
pass unchanged.
