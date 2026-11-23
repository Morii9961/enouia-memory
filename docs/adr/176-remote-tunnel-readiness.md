# ADR-MEM-176 — Tunnel-ready remote service, drafted connector grants and deploy bundle

Date: 2026-11-24. Status: Accepted (Runtime asked what Memory can prepare
without new authorization; Morii chose a quick-tunnel check on the main Vault);
synthetic validation.

## Context

The remote MCP service (ADR-MEM-133/158) had been tested only in process. Before
Morii connects ChatGPT, through a Cloudflare quick tunnel now and a named tunnel
or a VPS later, four gaps remained:

- **Chunked bodies.** The loopback HTTP driver accepted only `Content-Length`
  bodies and answered `Transfer-Encoding` with 501. The Linux reference relied
  on Caddy buffering. A tunnel such as cloudflared forwards without buffering and
  may send a chunked body, so a request ChatGPT sends could fail before it
  reached the service.
- **Grant drafting.** Creating the connector's read grant meant writing an
  `OwnerReadGrant` JSON by hand: every recorded revision of the chosen policies,
  sources, principal, ceiling, `follow_new_sources` and `purpose`. Hand-written
  JSON is easy to get wrong for a one-off.
- **Process-level evidence.** No test drove the shipped binary through
  ChatGPT's whole daily flow.
- **Deploy material.** The service file, proxy and tunnel snippets lived only
  inside prose, and there was no repeatable Linux bundle.

## Decision

- **Chunked request bodies.** The driver decodes `Transfer-Encoding: chunked`.
  The total is bounded by `MAX_INPUT_BYTES`. Chunk extensions and trailers
  (at most 64) are read and discarded, and bytes after the body are never read.
  It refuses:
  - a request carrying both `Content-Length` and `Transfer-Encoding` (400,
    RFC 9112 6.1);
  - any other coding (501);
  - malformed sizes or terminators (400).

  Responses are unchanged: one `Content-Length` response, then close.
- **`read-draft <config> <UTC-expiry> <plan-file>`.** This drafts the
  connector's grant from the service config:
  - the config's Agent, subject and project, ceiling and `source_read`;
  - every recorded revision of the config's `policy_ids`, and its `source_ids`;
  - a fresh grant ID, `follow_new_sources: true` and
    `purpose: "chatgpt_connector"`;
  - validity from now until the given expiry.

  It writes the reviewed-plan file as UTF-8 and never overwrites one. It prints
  the plan file and grant ID, and writes nothing to the Vault. Committing still
  needs `read-set ... --confirm-owner-read`. Because the grant follows the
  subject's windows, the config needs only one seed policy and its sources, not
  every policy.
- **Deploy templates.** `docs/deploy/linux/` holds four templates:
  - a hardened systemd unit;
  - a Caddyfile;
  - a named-tunnel `cloudflared` config;
  - a placeholder `remote.json` with a read grant and the connector section.

  `tools/deploy/package_linux.sh` builds the three release binaries on Linux
  from a clean checkout. It packs them with the templates and documents under
  `.local/dist/`, adds per-file and archive SHA-256, and deploys nothing.
  Docker is not provided: one binary under systemd avoids volume ownership and
  data-root checks inside a container.
- **Quick-tunnel runbook.** `docs/deploy/CHATGPT_QUICK_TUNNEL_CHECK.md` covers:
  - stopping Runtime;
  - a verified export;
  - drafting and confirming the grant;
  - `cloudflared tunnel --url`;
  - pointing `resource` and `issuer` at the tunnel;
  - serving, approving, submitting and checking in Runtime;
  - keeping or revoking afterwards.

## Consequences

- Tunnel routes work without a buffering proxy. Caddy keeps buffering.
- Morii reviews one generated plan instead of writing grant JSON. The config
  stays small.
- A quick tunnel's address changes on every start. `resource`, `issuer` and
  the ChatGPT connector must follow it, so it is for verification only.
- Registration stays capped at 16 clients. On a public address, strangers could
  fill the cap. That affects availability only; `oauth-revoke` clears it.
- No Runtime IPC or wire change, and no schema change.

## Verification

- **Chunked.** The tests cover:
  - a body split across chunks with a trailer, and a chunk extension, both
    served;
  - a following request on the same connection, ignored;
  - more than 256 KiB, refused with 413;
  - malformed, short or unterminated chunks, all refused before the endpoint;
  - both framings at once, refused with 400, and `gzip, chunked` with 501.
- **Daily flow through the real process** (`remote_daily_process`, Windows and
  Linux). A synthetic segmented Owner Vault is used. The process runs these
  steps in order:
  1. `read-draft` writes a plan with follow, purpose and the seed scope, and
     writes nothing to the Vault. A second draft to the same file is refused.
  2. `read-set` confirms it, and `read-check` reports `owner_read_grant`.
  3. `serve` runs as a child process.
  4. The client registers. `approve`, run as a separate process, issues the
     code, and the PKCE exchange returns tokens.
  5. `tools/list` includes search, read and both write tools.
  6. A conversation preflight reports that a window is needed and writes
     nothing.
  7. The conversation is submitted and forms one memory. Resubmitting it does
     not form another. A second conversation forms another memory.
  8. The next search shows both new memories, through the following grant.
  9. A theme preflight returns nothing. The theme then forms, and resubmitting
     it replays.
  10. After the process is killed, the reopened Vault holds exactly three new
      memories. The theme is an inference derived from the two others.
  11. The OAuth state file contains no code or token.
