# Owner-root Agent MCP pipe v1

2026-10-29, ADR-MEM-105 checkpoint 4. This is a distinct opt-in read protocol,
not an upgrade of the old six tools or existing Agent pipe profiles.

The frozen descriptor uses `memory-agent-pipe-4`, schema 2,
`automation-segmented-owner-v1`, wire capability `memory-agent-qualified-read-1`,
native capability `memory-agent-qualified-native-read-1`, source carrier
`captured-source-original-1`, MCP `2025-11-25` and Memory packet version 2.
Its compatibility hash binds the descriptor and normalized closed schema bundle.
Vault ID and every descriptor/hash field must match before a business call.
Native OS enrollment binds the actual kernel peer, principal and source-read
ceiling. ClientInfo, packet parameters, chosen protocol or self-reported PID do
not grant authority. The native backend reuses the one already leased Vault.

Five tools only: memory_search, memory_read, memory_source, memory_related and
memory_recall. Each declares closed input/output schemas and read-only,
non-destructive, idempotent, closed-world annotations. Requests use
`agent_qualified_read_request`; responses use `agent_qualified_read_response`.
The wire capability differs from the data-only native projection capability.
Unsupported version/capability/identity fields refuse before argument dispatch or
read-call counters. No write, Owner/System port or Workspace forwarding exists.

Search returns full qualified fields, Owner marks and optional scoped facets.
Read exposes bounded original history with a delegation/profile/snapshot/memory
bound cursor; each page assembles at most 32 history records and source metadata
entries. Counts describe the full permitted history and partial coverage is
explicit. Source excerpts need source-read. Safe receipts exclude global sequence.
Relations separate recorded lineage/entity mentions from theme suggestions;
same-conversation and semantic neighbors remain unavailable. Recall and native
Context use the actual same scoped response/trace, without reinforcement.

Lock/revoke/expiry/exit invalidates the delegation. The protected pipe checks the
retained OS peer before handshake and every call. Failure closes the connection;
unknown results retain the exact original packet/request key without automatic
resend or reconnect. The stdio edge only attaches to a native enrolled Host via
`--protocol automation-qualified-v1`; it opens no Vault and installs no config.

| Format / consumer | Support |
| --- | --- |
| Old schema-1, Host V2/V3 or segmented pipe profiles | Original semantics; cannot select this root |
| Owner root + pipe-4 / qualified-read-1 | Explicit five-tool read profile |
| Owner root + old Agent/Runtime capture profile | Existing early refusal |
| Unknown layout/carrier/hash or forged principal/scope | No business admission |
| Initializer import/attachment carrier | Not adopted; requires its published candidate and explicit compatibility update |

This contract does not claim initialization-carrier adoption, daily formation,
new write tools, Runtime UI/hook/pin, private initialization or real clients.
Synthetic independent-process acceptance and final evidence are recorded in
MCP-owner-agent-pipe.md after implementation and checks.
