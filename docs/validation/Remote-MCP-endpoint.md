# Remote MCP endpoint — synthetic validation (2026-11-08)

Scope: [ADR-MEM-129](../adr/129-chatgpt-remote-mcp-endpoint.md), branch
`claude/chatgpt-remote-mcp` from main `731edd5`. Synthetic roots and fake
tokens only. No network, listener, TLS, VPS, real Vault or ChatGPT client.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-host --locked --test remote_http` | 5 passed |
| `cargo test -p enouia-memory-host --locked --test remote_http_opening` | 1 passed |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p enouia-memory-host --all-targets --locked -- -D warnings` | clean |

The workspace-wide suite was not rerun: the change adds one new module and two
test files in the host crate, with no dependency, schema or integration-surface
change. Schema cross-check and Runtime surface checker do not apply.

## What the tests show

`remote_http.rs` (platform-neutral, fake session):

- `Config` refuses plain HTTP, query strings, a bare host, missing or non-HTTPS
  authorization servers.
- Protected resource metadata is served without a token at both well-known
  paths. Missing, forged, Basic, duplicated and query-string tokens, narrow
  scopes and foreign origins are refused before any session is built or any
  message reaches it.
- `initialize` issues one session ID bound to the bearer subject; the
  notification answers 202; another subject, a missing or unknown session, a
  missing or old protocol version, a second `initialize`, wrong content type
  or accept, a batch, duplicate keys and an oversize body are refused. GET is
  405. Only the three valid messages reached the session.
- A refused `initialize` leaves no session; the cap of four answers 503; a
  server-closed session and DELETE leave 404 behind; another subject cannot
  delete a session.
- An unknown outcome drops all sessions, keeps the exact original message and
  answers 503 afterwards without touching any session again.

`remote_http_opening.rs` (Windows, actual opening server on a synthetic
initialized root): initialize, the six-tool catalog and an actual
`session_start` recall pass through the endpoint with the original request ID;
an authority field is refused with -32602; the Vault snapshot is unchanged;
DELETE ends the session.

## Not shown

Real HTTPS, OAuth issuance, ChatGPT interoperability, write tools, capture,
deployment, performance and owner acceptance.
