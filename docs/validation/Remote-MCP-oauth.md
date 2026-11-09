# Remote MCP OAuth authority — synthetic validation (2026-11-09)

Scope: [ADR-MEM-132](../adr/132-remote-mcp-owner-oauth.md), branch
`claude/remote-mcp-oauth`, stacked on `claude/remote-mcp-http1` (PR #89).
Synthetic clients, a controllable clock and deterministic IDs. No network,
real ChatGPT client, real Vault or VPS.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-host --locked --test remote_oauth` (Windows) | 6 passed |
| `remote_http`, `remote_http1`, `remote_http_opening` (Windows) | 5 + 4 + 1 passed, after the driver became generic over `Handler` |
| `cargo clippy -p enouia-memory-host --all-targets --locked -- -D warnings` (Windows) | clean |
| `cargo fmt --all -- --check` | clean |
| Linux (WSL2 Ubuntu 24.04): see below | |

## What the tests show

- Configuration refuses a non-HTTPS or path-bearing issuer, a resource on
  another origin, and missing, prefix-less or HTTP redirect prefixes.
- Metadata advertises S256, `none` client auth and the token endpoint.
  Registration refuses missing, empty, foreign, fragment-bearing or non-string
  redirect URIs, secret-based auth, other grant or response types, non-object
  bodies and duplicate keys. Registration stops at 16 clients.
- The authorize page escapes `state` and sends CSP, frame and referrer
  protection. Unknown clients and unregistered redirect URIs get a page,
  never a redirect. A non-S256 challenge, a wider scope and a foreign resource
  redirect with `invalid_request`, `invalid_scope` and `invalid_target`.
- Without an approval code nothing is approved. Four wrong codes are counted;
  the fifth voids the code, after which even the real code fails. Expired
  codes fail. A fresh code works once (case-insensitive) and is consumed.
- A wrong PKCE verifier burns the code. An unknown client gets 401. A valid
  exchange returns Bearer tokens with `no-store`. The access token opens an MCP
  session through `Router`; both metadata documents are served there; at
  expiry the token stops verifying and MCP answers 401.
- Refresh rotates. Another client cannot use the token. Replaying the old
  refresh token, or a used authorization code, revokes every token in the
  family. Codes expire after two minutes. An unknown grant type is refused.
- The serialized state contains no code or token. It restores tokens and
  refuses unknown fields.

## Not shown

Interoperability with ChatGPT's current connector client, real HTTPS, the
approval-code command, persistence on disk and concurrent processes, and owner
acceptance.
