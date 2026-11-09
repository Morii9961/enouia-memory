# Remote MCP service on a Linux VPS

Reference for [ADR-MEM-133](../adr/133-remote-mcp-service-entry.md). It
describes how the pieces fit together. Deployment, the domain, and which real
Vault and scope to expose are Morii's decisions and are not made here. All
values below are placeholders.

## Layout

```text
ChatGPT ──HTTPS──▶ Caddy (TLS, :443) ──HTTP──▶ enouia-memory-remote serve (127.0.0.1:8787)
                                                 │
                         owner-only Vault root ◀─┤  (ext4/xfs/btrfs, 0700, service account)
                         owner-only state dir  ◀─┘  (OAuth digests, beside the Vault)
```

- One dedicated system account (for example `enouia`) owns the Vault root and
  the state directory and runs the service. That ownership is how the server
  authenticates the Owner. Nothing else runs as this account.
- The Vault must be an initialized Owner Vault (`SegmentedOwnerV1`) on a
  durable local filesystem, with a host id equal to `host_id`.
- The state directory lives outside the Vault root. The service refuses
  otherwise.

## Creating the Owner Vault (ADR-MEM-134)

As the service account, prepare an owner-data home outside any repository and
outside `/tmp`, then run the native bootstrap:

```sh
sudo -u enouia sh -c '
  home=/var/lib/enouia-memory/home
  mkdir -p "$home" && chmod 700 "$home"
  echo "Enouia owner data; never delete" > "$home/DO_NOT_DELETE.USER_DATA"
  printf %s "{\"format\":\"enouia-owner-data-home/1\",\"storage_class\":\"owner_data\",\"allow_development_cleanup\":false,\"allow_git_upload\":false,\"production_vault_created\":false,\"active_vault\":null}" > "$home/.enouia-owner-data.json"
  chmod 600 "$home/DO_NOT_DELETE.USER_DATA" "$home/.enouia-owner-data.json"
  enouia-memory-owner-bootstrap prepare "$home" --bind-current-owner-self
'
# then: enouia-memory-owner-bootstrap commit "$home" <operation_id> --bind-current-owner-self
```

The new Vault is `$home/vaults/<vault_id>`. Use its descriptor's `host_id` in
the service file. The Vault starts empty; filling it (policies, sources,
memories) is a separate, owner-authorized step.

## Service file (`/etc/enouia-memory/remote.json`, mode 0600)

```json
{
  "vault_root": "/var/lib/enouia-memory/home/vaults/vlt_00000000-0000-4000-8000-000000000005",
  "host_id": "dev_00000000-0000-4000-8000-000000000000",
  "state_dir": "/var/lib/enouia-memory/oauth",
  "bind": "127.0.0.1:8787",
  "resource": "https://memory.example.org/mcp",
  "issuer": "https://memory.example.org",
  "redirect_prefixes": ["https://chatgpt.com/"],
  "origins": ["https://chatgpt.com"],
  "agent_id": "prn_00000000-0000-4000-8000-000000000001",
  "subject_id": "sub_00000000-0000-4000-8000-000000000002",
  "project_id": null,
  "policy_ids": ["pol_00000000-0000-4000-8000-000000000003"],
  "source_ids": ["src_00000000-0000-4000-8000-000000000004"],
  "max_sensitivity": "private",
  "source_read": true,
  "profile": { "...": "an AttentionProfile document" }
}
```

The exposure scope (`subject_id`, `project_id`, `policy_ids`, `source_ids`,
`max_sensitivity`, `source_read`) is exactly what ChatGPT may read. The Vault
refuses anything an active policy does not cover. Unknown fields are refused.

## systemd (`/etc/systemd/system/enouia-memory-remote.service`)

```ini
[Unit]
Description=Enouia Memory remote MCP
After=network-online.target

[Service]
User=enouia
Group=enouia
UMask=0077
ExecStart=/usr/local/bin/enouia-memory-remote serve /etc/enouia-memory/remote.json
Restart=on-failure
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/enouia-memory
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

If the service stops because an outcome is unknown, it exits with an error and
stays down after the restart limit. Recover the Vault before starting it again.

## Caddy (`/etc/caddy/Caddyfile`)

```text
memory.example.org {
    reverse_proxy 127.0.0.1:8787 {
        # The service accepts only Content-Length bodies, so buffer requests.
        request_buffers 300KiB
    }
}
```

## Connecting ChatGPT

1. In ChatGPT's developer mode, add a connector with the MCP URL
   (`resource`) and OAuth.
2. When the authorize page asks for an approval code, run this on the server:
   `sudo -u enouia enouia-memory-remote approve /etc/enouia-memory/remote.json`
   Type the printed code into the page within 10 minutes.
3. ChatGPT then holds rotating tokens. Approving another client needs a new
   code.

## Operational notes

- Vault permits last five minutes and renew every four. Open MCP sessions end
  at renewal, and ChatGPT starts a new session with the same token.
- Requests are served one at a time; this is a single-owner service.
- Interoperability with ChatGPT's current connector client has not been
  tested yet.
