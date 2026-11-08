# ADR-MEM-129 — Remote Streamable HTTP endpoint for a ChatGPT connector

2026-11-08. Morii's real conversations happen in ChatGPT, not in Runtime. Asked
how they should reach Memory, Morii chose a ChatGPT developer-mode MCP connector
hosted on a VPS (Plus plan), no per-turn owner confirmation, Asia/Shanghai as
the default timezone. ChatGPT only connects to public HTTPS MCP servers
(Streamable HTTP, OAuth or no auth); it cannot reach local stdio or named pipes.

## Decision

Add `mcp::remote`, a transport-neutral Streamable HTTP endpoint (MCP
2025-11-25). The caller's HTTPS listener passes one parsed request and writes
back the returned response. This first slice serves the existing read-only
opening profile (ADR-MEM-126/127: six read tools, resources, prompts) only.

- **Resource.** `Config` takes the public `https://host/path` resource URL,
  at least one `https://` authorization server and the allowed `Origin`
  values. Plain HTTP, query, fragment and user info are refused.
- **Authorization.** OAuth 2.1 protected resource: RFC 9728 metadata is public
  at `/.well-known/oauth-protected-resource{path}` (and the root form) and
  names `memory.read`. Every MCP request needs exactly one
  `Authorization: Bearer` header. A missing header gets 401 with
  `resource_metadata`; an unverifiable one gets `error="invalid_token"`;
  missing `memory.read` gets 403 `insufficient_scope`. Tokens in the query
  string are never read. Issuer, signature, expiry and audience binding
  (RFC 8707) belong to the `Tokens` verifier port; unauthenticated mode does
  not exist.
- **Sessions.** `initialize` without a session header asks the caller's
  builder for one session bound to the bearer subject (builder `None` → 403).
  Only a successful result issues `Mcp-Session-Id` (128 random bits from the
  injected `IdSource`). Later requests need that header, the same subject and
  `MCP-Protocol-Version: 2025-11-25`; otherwise 400/404. At most four
  sessions; DELETE ends one; a session the server closes is dropped.
- **HTTP shape.** POST only with `Content-Type: application/json` and an
  `Accept` that allows JSON; one JSON-RPC object per body (no batches),
  duplicate keys refused, 256 KiB cap. Requests answer `application/json`;
  notifications answer 202. No SSE stream is offered: GET is 405. Every
  response carries `Cache-Control: no-store`. A foreign `Origin` gets 403.
- **Unknown outcome.** If a session reports an unknown outcome, the endpoint
  drops every session, keeps the complete first original message and answers
  503 to everything until the operator recovers and builds a new endpoint.
  Nothing is retried or replayed.

## Not in this slice

The HTTPS listener and TLS, the authorization server (client registration,
owner sign-in, token issuance), write tools, conversation capture from
ChatGPT, VPS deployment, moving any real Vault off this PC and ChatGPT
acceptance. Capture is still Runtime-only under ADR-MEM-102; letting a remote
Agent write conversations needs its own decision. Whether Plus can call write
tools is reported inconsistently by third parties and is unverified.

## Consequences

The remote boundary is testable without network or Windows pipes, and the
opening server runs unchanged behind it. Runtime adopts nothing; the Runtime
integration surface is unchanged. Synthetic evidence only:
[validation](../validation/Remote-MCP-endpoint.md).
