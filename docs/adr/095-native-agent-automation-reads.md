# ADR-MEM-95 — Scoped native Agent reads of automation V2

2026-10-26. Accepted within Morii's explicit continuous MCP development authorization.

Add a separate native Agent reader for automation-host-v2 / rule 3. Never call
System memory_page_native/detail_native by relabeling the principal. Reuse only
pure projection after a Vault Agent authorization gate has filtered all records,
history, receipts, provenance and counts. Legacy six tools retain schema-1 meaning.

A trusted native Owner registers an Agent, exact subject/project, selected source
IDs, sensitivity ceiling, independent source-read permission and expiry. The
nonserializable permit binds the exact current automation policy reference and
live Vault instance. Registration is at collection scope, not per-memory review.
It authenticates no JSON claims; OS enrollment remains the transport's separate
responsibility. No System capability, writer or Owner operation is granted.

Each read rechecks active current policy, primary/witness, source revision,
deletion, sensitivity and entire derivation/reversion closure before projection.
An inaccessible selection returns absence; hidden dependency IDs/edge counts
must not escape in dimensions. Policy changes invalidate permits, new snapshots
invalidate cursors, and lock/unlock invalidates old native readers. Expiry also
uses a five-minute monotonic ceiling. Revocation cannot revive a permit.

Output capability is memory-agent-read-1, effect agent_read, schema 2. It contains
Agent principal/scope hash and qualified original System formation, stored
receipts and separate event/valid/learned/source times. It is not Owner approval
or independent corroboration. Source text and metadata require independent native
source-read permission; ordinary memory evidence refs remain provenance.

The native foundation supplies scoped page/detail; ADR96 adds the explicitly
versioned five-tool pipe/stdio adapter in the same MCP-2 delivery. Existing
schema-1 Agent pipe still refuses V2. No schema downgrade, migration, Provider,
private data, Runtime change or deployment occurs. See [contract](../contracts/MCP_AGENT_READ_V2.md).
