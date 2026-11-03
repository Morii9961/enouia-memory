# ADR-MEM-115 — Native Agent note draft gate

2026-11-03. MCP-3 continuation after the Runtime capture wire and worker.

`memory_note` is an optional hint about an already captured Runtime source. It
must neither become another capture path nor borrow an existing read grant as
write permission. This slice establishes its native input gate before storage
or advertisement.

Native Owner setup explicitly grants a separate opaque AgentNotePermit with
the same bounded source/policy/sensitivity/lifetime ceilings as initialized
Agent reads, including source-read permission. Existing readers cannot create
or supply this permit. Host locking invalidates old note delegations just as it
invalidates readers. No packet chooses the principal, scope or Owner identity.

The request binds an exact source revision, request ID, at most 500 Unicode
scalars of quote hint and at most 2000 scalars / 8 KiB of optional interpretation.
The source must be a currently permitted native Runtime capture, at most 16 KiB.
The Host uses the existing pinned quote locator, then the Vault independently
checks exact source/role/evidence resolution under its current writer lock.
Repeated matches, forged evidence, deleted/unselected sources, stale policies,
revocation, expiry and another Vault instance refuse without publication.

The returned draft binds the original source quote and actual speaker/evidence
class, authenticated Agent provenance, delegation, current snapshot and stable
principal/request key. It is data, never a capability or success receipt. No
source, job, memory, Owner action or hint journal is written. The optional hint
cannot reclassify assistant text as a user assertion or Owner confirmation.

Durable submission, idempotent receipts, System consumption and a separately
negotiated MCP write profile follow in later slices. Pipe-5 remains read-only.
No schema/wire version changes in this native-only slice. See the
[native contract](../contracts/AGENT_NOTE_NATIVE_V1.md).
