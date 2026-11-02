# Native Agent local transport v1

2026-10-25. Frozen by [092-native-agent-local-transport.md](../adr/092-native-agent-local-transport.md).

Native `Enrollment::enroll(pid, Agent, scopes)` is called only by a trusted
operator/launcher. Its PID is configuration, never a wire claim. The retained
OS process object, account/session match and kernel peer verification authenticate
the enrolled process. Principal and ceilings are never accepted from MCP input.
Accepted ceilings: memory:read, source:read, memory:propose, context:read,
session:propose; nonempty, unique, no Owner/System scopes. Persisted current
record/project/source/sensitivity/deletion policies remain independently required.

An Agent pipe uses existing u8-tag/u32le-length framing on a separately selected
protected local pipe. Hello=1, Welcome=129, Call=2, Reply=130; all other tags are
refused, including Lock/Unlock. Hello/Welcome <=4096 bytes, Call <=256 KiB,
Reply <=1 MiB. JSON rejects duplicate decoded keys. One IO deadline bounds the
whole frame. No endpoint discovery or remote pipe is allowed.

Hello is the exact object returned by `mcp::native::hello(vault_id)`:
`protocol=memory-agent-pipe-1`, `recordSchema=1`, `layout=host-current-v2`,
`capability=memory-agent-six-tools-1`, `mcpVersion=2025-11-25`, `memoryVersion=1`,
`vaultId` and `compatibilityHash`. Equality rejects unknown fields, wrong Vault,
versions, layout or capability. The hash covers this descriptor, the existing
Host compatibility hash and normalized Memory IPC/common schema bytes. It is
compatibility metadata, not peer authentication. Welcome echoes the exact Hello;
connection generation is the nonserializable gate instance/serial, never a
resumable token. Each new socket consumes a fresh native Enrollment.

Call contains one unchanged MCP JSON-RPC message; Reply is exactly
`{"response": <MCP response or null>}`. Null acknowledges a notification only.
Initialize/initialized and six tools keep ADR80 and the complete named ADRs
087-host-scoped-agent-reads, 088-host-agent-proposals,
089-host-agent-local-context and 084-host-agent-checkpoints semantics.
Native admission is checked even for ping/list; the gate checks business calls
under its lifecycle exclusion. Invalid/expired/revoked/stale transport terminates.
A lost response poisons the client and preserves the original message; the stdio
adapter emits outcome_unknown with originalRequest and closes. A failed stdout
also closes. No automatic reconnection, resend or replacement key is permitted.

| Consumer / capability | schema-1 / host-current-v2 | schema-2 / automation-host-v2 (rule 3) |
| --- | --- | --- |
| Existing Memory IPC v1 / six MCP tools | Read/search/source under current Agent policy; pending proposal, limited offline Context and provisional checkpoint | Refuse root before record decode/query/count; no V2 Agent reader/writer |
| This native Agent pipe / stdio bridge | Same six tools, exact capability/hash, native process enrollment | Refuse; upgrading source pin alone grants nothing |
| Legacy Workspace / Owner writer | Existing Owner-only meanings, schemaVersion 1 / 58 commands | Refuse; no implicit migration, conversion or fallback |
| AutomationVault / AutomationHost | Refuse as an automation root; no legacy rewriting | Existing native System gate, witnessed transactions, automatic staging/reassessment |
| Core-7 memory-native-read-1 | No legacy Owner-history projection | System-gated native read only; not exposed to Agents by this pipe |
| Unknown-major raw-byte diagnostics | No additional authority | Diagnostic preservation only; no business reader/writer/count support |

Source Git pin, record schema, segmented storage format 2, Host publication,
Workspace IPC, Memory IPC/MCP and native capability are independent axes.
Agent speech remains Agent provenance, never inferred Owner/user confirmation.
No machine-format MCP adoption or mandatory per-memory owner review is claimed.
