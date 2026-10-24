# MV-8.2b · Host-scoped Agent reads

Date: 2026-10-23 (Asia/Shanghai). Baseline `c1499b79043f5af0d164edf182698151ddddd2cc`,
branch `codex/mv8-mcp-boundary`. Morii explicitly selected **授权 MV-8.2 本地合成开发**.
[ADR-MEM-87](../adr/087-host-scoped-agent-reads.md) fixes the read slice. The
separate MV-8.1j closeout branch and its outstanding graphical acceptance remain
preserved; this authorization does not reopen Computer Use or Runtime UI work.

The later [MV-8.2c proposal slice](MV-8-host-agent-proposals.md) adds actual
pending proposal writes. The read-only constructor and this report's original
read checks remain valid; context/checkpoint and external Agent enrollment are
not supplied by that follow-up.

## Delivered behavior

`HostService::mcp_read_server(Grant)` now creates a distinct in-process Agent
connection over the Host's existing bound Vault. The server borrows HostService;
there is no second Vault open, writer, root picker or owner Workspace forwarding.
It accepts only memory:read/source:read ceilings and advertises only the enabled
search/read/source tools. Write/context grants are refused at construction.

The embedding layer must authenticate/enroll the native Agent separately.
This API and Grant's constructor do **not** establish OS authentication. Existing
owner pipe framing/hello/native controls are unchanged and cannot enroll Agents.
Agent and window connections cannot be substituted for each other. They share
the same lifecycle gate, five-minute TTL, capacity, clock checks and lock/unlock
revocation; old Agent handles stay invalid after an unlock.

The domain port uses current persisted PolicyRecords and server-resolved record
metadata. Owner-only genesis gives Agents no access. memory:read grants no source
text. Direct missing/unauthorized IDs share not_found. Both current and requested
historical metadata must allow the read. Source records have no project field;
a project-limited grant cannot infer a source project from its memory citation.
Related-memory IDs and attachment availability are separately filtered.

Reuse of the existing index, governance view and raw locator resolver preserves
current tombstones, purge and provenance checks. Historical search also checks
the current memory's sensitivity/project before delivering snippets. Authority
and policy/deletion freshness are checked through delivery. No request is rerun.
Typed content-free audit append is mandatory before success; audit failure
returns audit_unavailable without a result. Error responses are nonretryable.
UTF-8 byte ranges must address valid boundaries; excerpt caps do not split a
character or return an empty page before EOF. Source object allocation is capped
at 64 MiB, result JSON at 128 KiB, source excerpts at the existing 8 KiB maximum.

## Actual checks

| Check | Result and boundary |
| --- | --- |
| Eight new actual Host/domain checks | Passed: default deny, persistent grant, independent source ceiling, principal/project isolation, policy revocation and sensitivity, history/current metadata, deletion of old pins/dependencies, expiry/lock/unlock/cross-kind refusal, UTF-8 bounds and audit failure |
| `cargo test -p enouia-memory-host --locked -- --test-threads=4` | **43 passed**, zero failures; includes existing service/session/Windows pipe/process/fault checks; doctests zero |
| `cargo clippy -p enouia-memory-host --all-targets --locked -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| Runtime surface regeneration/check and four Rust surface guards | Passed; 55 Workspace commands unchanged; aggregate `004590baf629e31483c4e220f63c5ad54cbaa86ae0b97f440179ea485b8ddfa7` logged in RUNTIME.md |

The first new-test invocation failed at synthetic Vault creation under the
restricted token (AccessDenied); the identical checks passed with normal
permissions. No ACL or product rule was weakened. All test roots are newly
created under a dedicated temporary base; cleanup is restricted to the owned
direct child. Test owner policy approvals and governance confirmations apply
only to invented fixture data, never to personal consent or records.

The three direct path dependencies (govern, import, index) already belonged to
Host's resolved Workspace build closure. Cargo.lock adds only those Host edges;
no package version, external dependency, schema or toolchain changed. Verification
is scoped to Host and the integration surface, not the full workspace, Provider,
history quality, Runtime or Activity. No graphical shell was started.

## Remaining exits and stable Runtime contract

Read binding is implemented and synthetically checked. context_get,
memory_propose and session_checkpoint still have no actual domain binding here.
The earlier six-tool fake-port/protocol checks do not establish those operations
over this Host. External native Agent enrollment/transport, real-client
compatibility, installation and personal acceptance remain pending.

Runtime's existing embedded integration remains valid. The historical-results
adoption pin stays `9d68d3d8707749ad3b0e2a36cfc97d71a27e712f`, workspace IPC v1,
**55 commands**, aggregate
`c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`.
This additive in-process Host API is not a request to switch Runtime's transport
or adopt MCP. No private Vault, real client, Provider send, cloud, migration,
multi-client deployment, manual status snapshot, push, merge or deployment occurs.
