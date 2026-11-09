# Remote MCP service entry — synthetic validation (2026-11-09)

Scope: [ADR-MEM-133](../adr/133-remote-mcp-service-entry.md), branch
`claude/remote-mcp-service` from main `05a123d`. A synthetic initialized Owner
Vault, a controllable clock and real loopback sockets. No VPS, proxy, TLS, real
Vault or ChatGPT client.

## Checks run

| Check | Windows | Linux (WSL2 Ubuntu 24.04, ext4) |
| --- | --- | --- |
| `remote_service` | 2 passed | 2 passed |
| `remote_oauth`, `remote_http`, `remote_http1` | 6 + 5 + 4 passed | 6 + 5 + 4 passed |
| `remote_http_opening` | 1 passed | Windows-only test |
| `cargo test -p enouia-memory-vault` | 20 binaries passed | segmented, segmented_agent and runtime_capture: 4 + 4 + 4 passed (now enabled on Linux) |
| `cargo clippy -p enouia-memory-host -p enouia-memory-vault --all-targets -- -D warnings` | clean | host clean |
| `cargo fmt --all -- --check` | clean | |

## What the tests show

- The service file is closed (an extra field is refused). A state directory
  inside the Vault root is refused before serving. An HTTP issuer makes
  `approve` fail.
- Over real sockets, with the server on one thread and a ChatGPT-style client
  on another:
  - Both metadata documents are served; the client registers.
  - The approve command, run beside the server through the locked state file,
    issues a code. The code authorizes; PKCE exchange returns tokens.
  - A forged token gets 401. The real one initializes an MCP session, lists
    six tools and gets an actual `session_start` recall from the Vault.
  - After the clock passes the renewal point, the old session answers 404 and
    the same token opens a new session on a fresh permit.
  - The state file contains none of the approval code, authorization code,
    access token or refresh token.
  - The Vault snapshot is unchanged.
- On Linux the segmented Vault now opens on ext4 with a 0700 root, and its
  existing suites pass.

## Not shown

A real Linux Owner bootstrap or migrated Vault, the VPS with Caddy and
systemd, ChatGPT interoperability, concurrency beyond one request at a time,
and owner acceptance.
