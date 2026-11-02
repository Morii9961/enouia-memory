# Segmented Agent MCP read contract v1

MCP-2c checkpoint 2. Implemented with synthetic local transport evidence in the
[validation report](../validation/MCP-2c-segmented-agent-reads.md).

## Admission and compatibility

Only schema 2 / `automation-segmented-host-v1` is supported by this explicit
`memory-agent-catalog-read-1` capability. The protected pipe hello uses
`memory-agent-pipe-3`, MCP `2025-11-25`, the exact Vault ID and the canonical
descriptor/schema compatibility hash. Old six-tool, HostV2 and HostV3 hello
messages refuse before business decoding or counts. A pin cannot upgrade a
client's capabilities or authority.

One existing leased Vault is shared through an Arc. A separate native Owner
registration creates the sealed Agent delegation, then retained OS process
identity enrolls the one pipe peer. `clientInfo`, root flags, principal, PID or
scope parameters grant no authority. Owner lock/unlock, disconnect, process
exit, expiry, rollback and withdrawal cannot revive an old connection. Native
Context can inspect the exact cached recall response only while its current
snapshot and delegation remain valid.

## Wire

Keep exactly five tools: `memory_search`, `memory_read`, `memory_source`,
`memory_related`, `memory_recall`. Each declares bundled input/output schemas,
read-only, non-destructive, idempotent and closed-world hints. Maximum 4096
unique JSON-RPC requests per admitted connection, still at most five minutes.
This explicit capability increases pagination call capacity; HostV2/3 retain
their existing 256-request bound. Repeated request IDs refuse.

Each closed packet has schema_version 2, kind `agent_catalog_read_request`,
capability, request_id, operation and arguments. Responses use
`agent_catalog_read_response`, the same operation/key, native principal,
scope_hash, typed data or a known non-retryable error, and fixed coverage.
Unknown transport outcomes retain the exact original packet and key. No
automatic replay, reconnect, replacement key or fabricated tool receipt.

Search takes the frozen CatalogFilter, CatalogCursor or null, limit 1–100,
and max_bytes 4096–262144. Filtering follows the published native catalog's
literal/alias text, form, state, knowledge, adoption, validity, attention,
entity, project, recorded theme derivation and four time axes. Counts precede
paging and follow Agent authorization. Cursor selection binds the snapshot,
withdrawal watermark, delegated scope, query/profile and evaluation time.
Oversized pages trim only with a progressing cursor; no empty nonterminal page.

Read, source, related and recall argument shapes preserve the corresponding
HostV3 read operations, except read/related require at least 4096 output bytes.
Read returns current/history items with full frozen record fields, producer/
model versions, authorized entities and source record fields. Raw source text
requires source-read; excerpts are bounded UTF-8 byte ranges of at most 8192
bytes. Source omission, history trimming or bounded links are explicitly marked.
Related exposes recorded provenance edges only at this baseline. Fact and
suggested graph projections are explicitly unavailable; stored annotations
retain authorized current endpoints and remain suggestions, with their stored
algorithm metadata. They do not become fact edges.

## Scalable recall and trace

Core-6's existing `select_recall` and legacy trace remain limited to 128
candidates. An explicit catalog selection API applies the same deterministic
ranking, conditions, validity, historical rescue and byte budget to at most
65536 already-authorized current candidates. It performs no IO or reinforcement.
It does not rank a truncated first page or independently rank chunks.

The catalog output carries all selected items and the actual rendered context.
Its trace keeps exact considered/selected/rendered-byte values, but at most 128
excluded identities. `trace_coverage` declares total excluded, returned count,
completeness and exact in-scope counts by exclusion reason. There is no claim
that an incomplete list is a complete legacy trace. Native Context receives
the identical computed response rather than a separate simulation. A later
snapshot, deletion, policy change or lock refuses the cached response.

Stored claims, quotes, occurrence, time precision/basis/timezone, adopted/staged
status and language remain unchanged. Subject statements are not independently
verified facts. Agent provenance and model hints cannot become Owner actions.
No read changes attention, source history, business commits or durable usage.

Runtime hook capture, live formation, preflight writes, reports/forget requests
and session resources remain later slices. Only Runtime will capture daily
turns; missing timezone defaults to Asia/Shanghai with explicit default-source
provenance. No private initialization is required for synthetic verification.
