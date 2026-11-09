# ADR-MEM-133 — Runnable remote MCP service for one Owner Vault

2026-11-09. Fourth slice of the ChatGPT connector route (ADR-MEM-129 to 132,
Linux VPS). Morii authorized continuous development along the route. The
endpoint, HTTP driver, OAuth authority and Linux platform layer existed, but
nothing assembled them into a process.

## Decision

Add `mcp::remote::service` and the `enouia-memory-remote` binary:

- **`serve <config.json>`** opens one initialized Owner Vault, binds the
  loopback driver and serves `Router` (OAuth authority, then the read-only
  opening profile). **`approve <config.json>`** prints a one-time approval
  code for the authorize page.
- **Owner authentication.** The process runs as the OS account that owns the
  0700 Vault root. The Vault already refuses roots that are not owner-only, and
  the Owner is the Vault descriptor's `created_by`. No password or Owner token
  exists on the server.
- **Exposure scope.** The service file names the agent principal, subject,
  project, policies, sources, maximum sensitivity and source reads. That is
  exactly the scope requested from the Vault, which refuses anything an active
  policy does not cover. The file has closed fields.
- **Permits.** Vault agent permits last at most five minutes. Each generation
  takes one permit valid for five minutes and serves until four minutes have
  passed. Then the generation ends: its sessions are dropped, the permit is
  revoked, and clients reinitialize with the same token. The HTTP driver now
  polls while idle (every 50 ms), so renewal does not wait for traffic.
- **OAuth state file.** It lives in an owner-only directory outside the Vault
  root; the service refuses a state directory inside the Vault. Every request,
  and the approve command, holds an exclusive file lock, loads the state, and
  writes it back atomically (temp file, then `replace_durable`) only if it
  changed. The file holds digests only.
- **Unknown outcome.** If the endpoint halts, the service revokes the permit
  and exits with an error. The operator recovers the Vault before restarting.
- **Linux Vaults.** Segmented Vaults now also open on Linux when the root is on
  ext4, xfs, btrfs, zfs or f2fs (not tmpfs) and is owner-only. Before, they
  required Windows/NTFS. Host-authority (MV-8 v2) mode stays Windows-only. The
  segmented, segmented-agent and runtime-capture Vault suites now also run on
  Linux.

A deployment reference (service file, systemd, Caddy, connecting ChatGPT) is
in [REMOTE_MCP_LINUX](../deploy/REMOTE_MCP_LINUX.md).

## Not in this slice

Creating or migrating a real Vault on Linux (the Owner bootstrap is
Windows-only), deploying to the VPS, choosing the real exposure scope, write
tools and conversation capture (an ADR-MEM-102 amendment), and a real ChatGPT
connection test. All of these need Morii's decisions.

## Consequences

Evidence is synthetic: [validation](../validation/Remote-MCP-service.md).
