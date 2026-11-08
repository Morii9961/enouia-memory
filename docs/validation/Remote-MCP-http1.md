# Remote MCP loopback HTTP/1.1 driver — synthetic validation (2026-11-08)

Scope: [ADR-MEM-130](../adr/130-remote-mcp-loopback-http1.md), branch
`claude/remote-mcp-http1` from main `96ea547`. Fake session and fake tokens
over in-memory streams and Windows loopback sockets. No proxy, TLS, VPS, real
Vault or ChatGPT client.

## Checks run

| Check | Result |
| --- | --- |
| `cargo test -p enouia-memory-host --locked --test remote_http1` | 4 passed; 5 repeated runs all passed |
| `cargo test -p enouia-memory-host --locked --test remote_http --test remote_http_opening` | 5 + 1 passed (unchanged endpoint) |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -p enouia-memory-host --all-targets --locked -- -D warnings` | clean |

The workspace-wide suite was not rerun: one new host module plus tests, no
dependency, schema or integration-surface change. Not compiled for a
non-Windows target (only the Windows toolchain target is installed).

## What the tests show

- Wildcard, public and IPv6-unspecified addresses are refused by
  `bind_loopback`.
- A well-formed initialize gets the session header, `Cache-Control`,
  `Content-Length` and `Connection: close`. A notification answers 202 with
  length 0. Public metadata works through the driver. With two pipelined
  requests in one write, only the first is answered.
- 18 malformed framings are refused with the planned status and an empty body
  before any session is built: HTTP/1.0, double space, absolute target,
  missing or duplicate Host, chunked coding, missing, signed, duplicate,
  oversize or overflowing length, a folded line, a control character, a bad
  header name, a short body, an oversize head, too many headers, a truncated
  head. An empty connection writes nothing.
- On real loopback sockets, sequential connections run initialize and then a
  session call. An idle connection costs one timeout and does not block
  later clients. A forged token gets 401 with the metadata challenge. The
  loop stops when asked.

## Not shown

Behaviour behind a real proxy, TLS, ChatGPT interoperability, concurrency and
throughput, non-Windows builds, a real Vault and owner acceptance.
