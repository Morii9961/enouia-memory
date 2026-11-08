# ADR-MEM-130 — Loopback HTTP/1.1 driver behind a TLS proxy

2026-11-08. Second slice of the ChatGPT connector route (ADR-MEM-129). Morii
continued the route and will host on a VPS. ADR-MEM-129 left the listener out.

## Decision

Add `mcp::remote::http1`, a small blocking HTTP/1.1 driver for the existing
`Endpoint`. TLS, certificates and the public port belong to a reverse proxy on
the same machine (for example Caddy). The driver never faces the internet.

- **Loopback only.** `bind_loopback` refuses any address that is not loopback.
- **One request per connection.** Read one request, answer it with
  `Content-Length` and `Connection: close`, close. Connections are served one
  at a time with read/write timeouts; a broken connection never stops the
  loop, a failed `accept` does. MCP sessions live in the `Endpoint`, not in TCP
  connections, so closing is harmless.
- **Narrow framing.** HTTP/1.1 only (505 otherwise). Origin-form target only.
  Exactly one `Host`. Header names must be tokens; control characters and
  folded lines are refused. At most 16 KiB of head and 64 headers (431).
  `Transfer-Encoding` of any kind gets 501, so the proxy must buffer request
  bodies. POST without `Content-Length` gets 411. Duplicate, signed or
  non-decimal lengths get 400. Bodies over 256 KiB get 413 before they are
  read. Bytes after the declared body are dropped, never read as a second
  request.
- No new dependency. The host `Cargo.toml` and the Runtime integration surface
  are unchanged.

## Not in this slice

A runnable remote entry that opens a real Vault, owner authentication on the
server, the OAuth authorization server, write tools and capture, and proxy or
VPS configuration. The Vault's supported production platform is Windows/NTFS;
on other systems the platform layer is a test fallback (no ACL inspection or
restriction). Which operating system the VPS runs is Morii's decision and
decides whether the Vault needs real non-Windows support.

## Consequences

The endpoint can now run over real sockets. It is tested on Windows loopback;
it uses only `std::net` and was not compiled for a non-Windows target here.
Synthetic evidence only: [validation](../validation/Remote-MCP-http1.md).
