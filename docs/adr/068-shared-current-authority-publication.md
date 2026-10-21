# ADR-MEM-68 — Shared CURRENT publication for opt-in Host roots

Date: 2026-10-18. Scope: owner-authorized MV-8.1c, native Windows library.

## Decision

Add optional, non-null `host_authority_hash` to CURRENT. Legacy serialization
omits it. Host mode writes content-addressed immutable authority under
`vault/host-authority/`; the SAME flushed CURRENT replacement selects data and
authority. Never call two separate file replacements atomic.

`HostCoordinator` holds the Host OS lease. Each operation obtains the writer
lock, reads authority afresh, verifies the pinned data closure, checks native
identity and quiescence before replay or writes, and publishes under that lock.
Published commits record the native primary identity. Ambiguous publication
returns outcome_unknown; exact retries replay without advancing authority twice.

Explicit bootstrap writes a bound `config/host-mode.json` gate first. Missing or
corrupt marker/CURRENT/authority never initializes or selects a fallback. Partial
bootstrap stays fenced pending a future explicit repair route. Ordinary commit,
recovery adoption, purge, sweep, old-format export and restore reconciliation
refuse Host mode. Older binaries are unsupported: strict CURRENT readers reject
the added field, but owner filesystem access or manually restoring an old backup
is not prevented. The authority-only pilot and this mode cannot silently mix.

The digest covers the descriptor and actual verified bytes of each manifest,
segment, retained revision and non-purged object in the pinned closure, plus
commit identity and policy/deletion watermarks. Authority and CURRENT are
excluded to avoid a cycle. Audit, journals, indexes, secrets and platform metadata
are outside this data digest. It is NOT a complete export or independent target
restore proof. Purged content follows committed tombstones.

Native owner approval/identity remain caller duties. No Host executable,
authenticated IPC, Runtime activation, MCP transport, production migration,
cloud/cross-host consensus or power-loss proof is added. Legacy unmanaged roots
retain their existing path.
