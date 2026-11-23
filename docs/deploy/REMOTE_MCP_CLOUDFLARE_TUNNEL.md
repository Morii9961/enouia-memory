# Remote MCP service on this PC behind Cloudflare Tunnel

Reference for running the ADR-MEM-133/158 remote service on Morii's Windows PC
before any VPS deployment. ChatGPT reaches it through a Cloudflare Tunnel bound to
Morii's own domain. All values are placeholders: `memory.example.org` stands for
the hostname Morii chooses on his domain. Nothing here is deployed until Morii
authorizes it.

## Layout

```text
ChatGPT ──HTTPS──▶ Cloudflare edge (memory.example.org)
                       │  tunnel (outbound from this PC, no open inbound port)
                       ▼
               cloudflared ──HTTP──▶ enouia-memory-remote serve (127.0.0.1:8787)
                                         │
                       Morii Vault root ◀─┤  (the registered home on this PC)
                       OAuth state dir  ◀─┘  (outside the Vault root)
```

- TLS ends at Cloudflare. The service listens on loopback only, and the tunnel
  forwards to it.
- **One process opens the Vault at a time.** While `serve` runs, Runtime's
  registered entry and any backup export must be stopped, and the other way round.

## Choosing a route (decide when the domain exists)

Morii runs his own DNS service, outside Cloudflare. A Cloudflare **named tunnel**
needs the domain's zone hosted at Cloudflare. On the free plan that means moving
the nameservers; adding only a CNAME needs a Business plan. Two routes:

- **A. Verify on this PC (this document).** Delegate the new domain, or a
  subdomain zone of it, to Cloudflare nameservers. Then use the tunnel below. It
  needs no inbound port and no public IP.
- **B. Go straight to the VPS.** Keep the existing DNS service and add an `A`
  (and `AAAA` if used) record for `memory.example.org` pointing at the VPS. Caddy
  obtains the certificate itself. No tunnel or Cloudflare account is needed.
  Follow the [Linux reference](REMOTE_MCP_LINUX.md); that route also moves the
  Vault primary to Linux.

The rest of this document is route A.

## What Morii does himself (route A, once the domain exists)

1. Add the domain to a Cloudflare account and switch its nameservers as
   Cloudflare instructs.
2. Install `cloudflared` and sign in. The sign-in opens a browser for the
   Cloudflare account:

   ```powershell
   winget install --id Cloudflare.cloudflared
   cloudflared tunnel login
   ```

The remaining steps need no account credentials typed into a session.

## Tunnel

```powershell
cloudflared tunnel create enouia-memory
cloudflared tunnel route dns enouia-memory memory.example.org
```

`%USERPROFILE%\.cloudflared\config.yml`:

```yaml
tunnel: enouia-memory
credentials-file: C:\Users\<user>\.cloudflared\<tunnel-id>.json
ingress:
  - hostname: memory.example.org
    service: http://127.0.0.1:8787
  - service: http_status:404
```

Run with `cloudflared tunnel run enouia-memory`. Optionally install it as a
service with `cloudflared service install` once verification is done.

## Service file

Same closed shape as the [Linux reference](REMOTE_MCP_LINUX.md), with Windows
paths outside every repository:

- `vault_root`: the registered Vault root;
- `state_dir`: a protected directory beside it;
- `resource`: `https://memory.example.org/mcp`;
- `issuer`: `https://memory.example.org`;
- `redirect_prefixes`: `["https://chatgpt.com/"]`;
- `origins`: `["https://chatgpt.com"]`.

For reads that survive formation windows, select an Owner read grant
(`read_grant_id`, [workflow](CONNECTOR_READ_GRANTS.md)). Morii decided its
ceiling is `highly_sensitive`, and ADR-MEM-159 lets one grant span policies of
different maxima. For writes, add the `connector` section (ADR-MEM-158) with
`"max_sensitivity": "highly_sensitive"`.

## Start and connect

1. Stop Runtime's access to the Vault. Make a verified native export (see
   [backup](SEGMENTED_MORII_BACKUP.md)).
2. Plan and set the read grant (`read-plan`, then
   `read-set ... --confirm-owner-read`). Then run `read-check`.
3. Run `enouia-memory-remote serve remote.json` and `cloudflared tunnel run
   enouia-memory`.
4. In ChatGPT developer mode, add a connector with
   `https://memory.example.org/mcp`. When the authorize page asks, run
   `enouia-memory-remote approve remote.json` on this PC and enter the code.
5. First call `memory_submit_conversation` with `"preflight": true`, then for
   real. This also shows whether the ChatGPT plan can call write tools.
6. Optionally, once some daily memories exist, ask ChatGPT for a theme across them
   with `memory_submit_theme` (preflight first). Themes stay ChatGPT's inference,
   linked to the memories they cite ([ADR-MEM-174](../adr/174-chatgpt-themes.md)).

## Not covered

The VPS move, a permanent Windows service setup, monitoring, and keeping
ChatGPT's read scope in step with new capture windows. The configured read scope
lists policies explicitly, so memories formed under later windows need a grant
or scope update before ChatGPT can read them.
