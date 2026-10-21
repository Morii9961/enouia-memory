# ADR-MEM-66 — Host authority and platform transaction contracts

Date: 2026-10-18. Status: Adopted (contract), MV-8.1a.

Morii explicitly authorized MV-8 development. MV-7 real Provider acceptance
remains pending; this decision does not replace it or activate real migration.

## Problem

ADR-MEM-46 excludes another embedded Core on the same Vault by holding its
`indexes/host.lock`. It cannot fence another host with a copied Vault. ADR-MEM-49
requires a single primary, an explicit migration and rejection of the old
primary. The current store has no primary epoch or migration protocol.

## Decision

Freeze the additive [Host authority v1](../design/HOST_AUTHORITY_V1.md) model,
strict schemas and platform transaction ports before changing any store or
transport. An authority binds one Vault to one host and a positive monotonic
primary epoch. A migration first quiesces writes at an exact checkpoint, then
compares the target's independently verified checkpoint, then promotes exactly
one new epoch with a retained, exact-plan receipt. Abort is allowed only while
quiesced; promotion cannot be undone by replaying an old abort or plan.

The checkpoint includes commit ID/sequence, complete snapshot digest and policy
and deletion watermarks. Ordinary writes must atomically advance data and
authority under the same exclusive transaction. A cached permit, local mutex or
standalone successful comparison is not a write capability. An unreachable
authority is unavailable, never permission to use a local second primary.

Storage adapters must supply durable atomic publication, owner isolation,
process exclusion and exact checkpoint verification. Reuse Memory's native
`SecretStore` port for Provider secrets; do not introduce serialized secrets or
move Windows credential storage into the transport-neutral model. Windows and
future cloud adapters need separate evidence for these obligations.

## Activation and evidence limits

This slice implements pure transition rules, failure tests and adapter contracts.
It does not write an authority file, alter CURRENT/catalog records, fence today's
embedded Core/CLI, start a Host process, open an IPC listener, expose MCP or
authenticate a client. Existing Vault and workspace IPC v1 remain unchanged.
Runtime must explicitly adopt the future Host before its embedded writer is
retired; an unavailable Host must never cause fallback to embedded writing.

Actual OS/process exclusion, transactional data/authority publication, power-loss
recovery, platform credentials, authenticated IPC and Runtime disconnect/reconnect
are later MV-8.1 implementation gates. MCP and a real client's compatibility
follow those gates. No synthetic authority model is cloud migration proof.
