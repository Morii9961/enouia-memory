# ADR-MEM-132 — Single-owner OAuth authority for the remote endpoint

2026-11-09. Third slice of the ChatGPT connector route (ADR-MEM-129/130,
Linux VPS per ADR-MEM-131). ChatGPT connects to a remote MCP server with OAuth.
ADR-MEM-129 left a `Tokens` port for verification but nothing that issues
tokens. Morii authorized continuing along the route.

## Decision

Add `mcp::remote::oauth::Authority`. It is a transport-neutral OAuth 2.1
authorization server for one owner, on the same origin as the resource, and it
implements `Tokens`. `Router` serves the Authority's paths first and the MCP
`Endpoint` second. The HTTP/1.1 driver now serves any `Handler`.

- **Discovery.** RFC 8414 metadata at
  `/.well-known/oauth-authorization-server` advertises authorization code with
  S256 PKCE, refresh tokens, public clients (`none`) and the `memory.read`
  scope.
- **Registration (RFC 7591).** Registration takes JSON with 1 to 4 redirect
  URIs, each under a configured `https://host/` prefix (for ChatGPT,
  `https://chatgpt.com/`) and without a fragment. Only `none` auth,
  `authorization_code`/`refresh_token` grants and the `code` response type are
  accepted. At most 16 clients; duplicate JSON keys are refused.
- **Owner approval.** No password exists. The owner gets a one-time approval
  code on the server (10 Crockford base32 characters, valid 10 minutes, only
  its digest stored) and types it into the authorize page. Five wrong entries
  void it. Each code approves exactly one authorization. An unknown client or
  an unregistered redirect URI gets an error page and is never redirected.
  Other request errors redirect with the OAuth error, `state` and `iss`.
  The page sets CSP with `frame-ancestors 'none'`, `X-Frame-Options: DENY`
  and `Referrer-Policy: no-referrer`, and escapes every echoed value.
- **Tokens.** Authorization codes last 2 minutes and are single-use. A failed
  exchange burns the code. Access tokens last 1 hour; refresh tokens last
  30 days and rotate on every use. Replaying a consumed code or a rotated-away
  refresh token revokes the whole grant family. All codes and tokens are
  256-bit random values; the state keeps only their SHA-256 digests. `resource`
  (RFC 8707), when given, must be the configured MCP URL, and `scope` may only
  be `memory.read`.
- **Persistence.** `State` is serde-serializable with closed fields and
  contains digests only. The caller decides where it lives. The runnable entry
  will keep it in an owner-only directory next to, not inside, the Vault.

## Not in this slice

The runnable Linux entry: process wiring, the approval-code command, state
file locking and writes, opening a real Vault and owner authentication. Also
write tools and capture, and VPS or proxy configuration. Whether ChatGPT's
current connector client matches every detail (for example the exact redirect
URI and the use of `resource`) is unverified until a real connection test.

## Consequences

Evidence is synthetic: [validation](../validation/Remote-MCP-oauth.md).
