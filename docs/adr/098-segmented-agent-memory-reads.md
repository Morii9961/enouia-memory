# ADR-MEM-98 — Scoped Agent reads of segmented roots

2026-10-26. MCP-2c. Depends on the published segmented contract b2c5c1a.
The first implementation checkpoint (`ec82a28`) delivers only the native Vault
delegation gate. Its validation does not advertise MCP tools. The second
checkpoint below binds the tools and records its own separate evidence.

Use a separate sealed native Agent permit for automation-segmented-host-v1,
schema 2, memory-agent-catalog-read-1. Bind the live Vault instance, native
Agent principal, exact selected policy references and source IDs, subject,
project, sensitivity/source-read ceilings and a five-minute maximum lifetime.
No client field or System read port grants this authority.

Reuse only the storage's verified-current/pure eligibility collector. Narrow
it through the Agent permit before text, labels, counts, edges and ranking.
Historical revisions and derivation/reversion closures must independently fit
current policy/source/deletion and delegated scope. Suggested annotations expose
only current authorized targets, without hidden-target counts or stored edits.

Use the existing single leased Vault, protected local peer enrollment and
explicit version/hash handshake. The MCP bridge opens no writer. Keep actual
Context results and no-reinforcement/unknown-packet semantics. Large result sets
require bounded output and explicit trace coverage; they cannot silently exceed
the existing bounded-root trace schema. All five tools need real synthetic
transport evidence before the capability is reported delivered.

Runtime-only capture/default timezone remain the next ingress slice; no private
root initialization, Provider/embedding, Runtime edit, remote opening or deploy.

The second checkpoint binds the five tools through memory-agent-pipe-3 and
explicit automation-catalog-v1 stdio attach. It reuses one existing leased Vault,
adds an independent native Agent Host lifecycle and a separate Core-6 catalog
selection entry point. Legacy selection stays capped at 128; catalog selection
ranks the full authorized set, with at most 128 exclusion identities on the wire
and exact reason counts/coverage. See the [wire contract](../contracts/MCP_AGENT_CATALOG_READ_V1.md)
and [actual validation](../validation/MCP-2c-segmented-agent-reads.md).
