# ADR-MEM-197 — OAuth accepts ChatGPT's extra scopes and announces `iss`

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation. Interop source: OpenAI's Apps SDK authentication guide,
developers.openai.com/apps-sdk/build/auth, read on 2026-11-24.

## Context

The guide says three things relevant here:

- ChatGPT sends `resource=<MCP server URL>` on both the authorization and the
  token request.
- ChatGPT requests the scopes a server advertises, plus OIDC scopes when
  those are advertised.
- ChatGPT uses the stable redirect `https://chatgpt.com/connector_platform_oauth_redirect`
  only when the server sets `authorization_response_iss_parameter_supported`
  and returns `iss` in every authorization response. Otherwise it uses a
  per-connector `https://chatgpt.com/connector/oauth/{callback_id}`.

Three details of our server put the first connection at risk.

- **Scopes.** The authorization and token endpoints refused any scope this
  server does not offer, for example `offline_access` or the write scope on
  a read-only server, with `invalid_scope`. Yet issued tokens always carry
  exactly the configured scopes, and the requested scope was only checked,
  never used to narrow. So one extra scope from ChatGPT would have stopped
  the hookup for no protection.
- **Resource.** It had to match the configured URL byte for byte, so a
  trailing-slash difference would also stop the hookup.
- **`iss`.** Every authorization redirect already carried `iss`, but the
  metadata did not say so.

## Decision

- **Scopes.** A scope request is accepted when it names at least one
  configured scope. Names are split on any whitespace, and unknown names are
  ignored, as RFC 6749 §3.3 allows. The token response's `scope` already
  states what was granted. A request naming no configured scope is still
  `invalid_scope`.
- **Resource.** It matches with or without one trailing slash, at both
  endpoints. Any other URL is still `invalid_target`.
- **Metadata.** The authorization server metadata now includes
  `authorization_response_iss_parameter_supported: true`. That is true of
  every redirect the server sends, error redirects included.
- **Unchanged.** Token endpoint authentication stays `none` only, with
  public PKCE clients. Configured redirect prefixes stay as they are; the
  usual `https://chatgpt.com/` covers both redirect forms.

## Verification

`remote_oauth` checks the following:

- A request for `memory.read  memory.write offline_access` (with a double
  space) on a read-only server reaches the approval page.
- A request for `memory.write offline_access` alone is `invalid_scope`.
- The resource with a trailing slash reaches the approval page.
- A foreign resource is still `invalid_target`.
- The metadata announces `iss` support.

The other remote suites, including the real-process daily flow, pass
unchanged.
