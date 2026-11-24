# ADR-MEM-204 — `set-origin` points the config at a new public origin

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

A Cloudflare quick tunnel gets a new random origin each time it starts.
Before every check, Morii had to hand-edit two JSON fields in the private
service config: `resource` (the origin with `/mcp`) and `issuer` (the bare
origin). A typo there shows up only later, as an OAuth `invalid_target` in
ChatGPT.

## Decision

`enouia-memory-remote set-origin <config> <https://host>`:

- **Input.** It accepts an `https://` origin only: no path, query, fragment,
  user info or spaces.
- **Rewrite.** It sets `resource` to the new origin plus the path the
  current `resource` already has (usually `/mcp`), and `issuer` to the
  origin. Every other field is left as it is.
- **Validation.** It checks the result with the same OAuth and endpoint
  constructors `serve` uses before writing anything. A refused origin leaves
  the file unchanged.
- **Write.** It writes the file beside the config, then renames it over the
  config, and prints the new `resource` and `issuer`.
- **Boundaries.** It does not open the Vault, touch OAuth state or start
  anything. Clients registered under the old origin still need to be
  re-added in ChatGPT, as before.

## Verification

`remote_read_admin_cli` covers three cases:

- `set-origin https://quiet-river-1234.trycloudflare.com` prints and writes
  the new resource and issuer. All other fields are equal before and after.
- `http://…`, an origin with a path, and a bare `https://` are each refused.
- After a refusal, the file is byte-for-byte the same.
