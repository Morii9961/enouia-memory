# Tunnel-ready remote service: synthetic validation (2026-11-24)

Scope: [ADR-MEM-176](../adr/176-remote-tunnel-readiness.md), branch
`claude/remote-tunnel-readiness`. The checks use synthetic Vaults and real
loopback sockets, with the real `enouia-memory-remote` process for the daily
flow. No Cloudflare tunnel, ChatGPT client, VPS or real Vault was involved.

## Checks run

| Check | Windows | Linux (WSL2 Ubuntu 24.04) |
| --- | --- | --- |
| `remote_http1`, including chunked framing | 5 passed | 5 passed |
| `remote_daily_process` (real binary, daily flow) | 1 passed | 1 passed |
| `remote_service`, `remote_read_admin_cli`, `remote_connector` | 4 + 1 + 2 passed | 4 + 1 + 2 passed |
| `cargo test -p enouia-memory-host --no-fail-fast` | all passed (one earlier run hit the known intermittent `opening_pipe` timing failure; three reruns and the full rerun passed) | |
| `cargo clippy -p enouia-memory-host --all-targets -D warnings` | clean | |
| `cargo fmt --all -- --check` | clean | |
| Runtime surface | regenerated and logged | |
| `tools/deploy/package_linux.sh` | | release build, 7.2 MB bundle with SHA-256 |

## What the tests show

- **Chunked framing.**
  - A chunked body with a trailer is decoded and reaches the endpoint, and so is
    one with a chunk extension.
  - Bytes after the body are not read as another request.
  - A body over 256 KiB is refused with 413.
  - Malformed sizes, missing CRLFs and truncated or unterminated bodies are
    refused before the endpoint.
  - `Content-Length` together with chunked is refused with 400, and any other
    coding with 501.
- **Daily flow through the real process.**
  1. `read-draft` writes a plan for a following grant labelled
     `chatgpt_connector`, scoped to the seed policy and sources. It leaves the
     Vault unchanged and refuses to overwrite the plan file.
  2. `read-set` confirms the plan, and `read-check` reports `owner_read_grant`.
  3. With `serve` running as a child process, the client registers. The
     `approve` command, run as its own process, issues the code. Authorization
     and the PKCE exchange return tokens.
  4. `tools/list` includes search, read and both write tools.
  5. A conversation preflight reports that a window is needed and writes
     nothing.
  6. Two conversations form one memory each, and a resubmission forms nothing
     more.
  7. The next search returns both memories through the following grant.
  8. A theme preflight returns nothing. The theme then forms, and a resubmission
     replays it.
  9. After the process is killed, the reopened Vault holds exactly three new
     memories. The theme is an inference derived from the other two.
  10. The OAuth state file holds no code or token.

## Not shown

These were not checked:

- a Cloudflare quick or named tunnel in front of the service;
- ChatGPT's own connector client and plan limits;
- the VPS route;
- the real Vault;
- Morii's acceptance.
