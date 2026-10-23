# MV-8.2a · Restricted MCP stdio boundary

Date: 2026-10-23 (Asia/Shanghai). Source baseline
`6c2b9999a7d978e8eba7fb06149eb8a202a60e20`; branch
`codex/mv8-mcp-boundary`. The owner continued the explicit MV-8.2 local synthetic
proposal. [ADR-MEM-80](../adr/080-restricted-mcp-stdio-boundary.md) freezes this
slice before implementation. No Computer Use or product UI work is performed.

Subsequent explicit MV-8.2 authorization and the actual in-process search/read/
source binding are recorded in [MV-8.2b](MV-8-host-scoped-agent-reads.md). This
report retains the earlier protocol-only evidence; it does not establish domain
binding for the other three tools or native external Agent authentication.

## Implemented boundary

The Host crate now contains an opt-in library adapter for MCP **2025-11-25**.
It implements UTF-8 newline-delimited JSON-RPC, initialize/initialized, ping,
tools/list and tools/call. Input is capped at 256 KiB, output at 1 MiB, and a
session at 256 distinct request IDs. Duplicate decoded JSON fields, batches,
invalid IDs, reused RPC IDs and incomplete/oversized framing are refused.
EOF, output failure, revocation or an uncertain backend reply ends the session.

The six tools reuse existing **Memory IPC v1**, which is distinct from the
unchanged 55-command Workspace IPC. Each tool accepts the existing closed full
request envelope, preserving requestId and write idempotencyKey. Advertised
schemas bundle common definitions and need no external schema fetch. The legacy
memory_update alias is callable but not advertised; its existing revise/
supersede-only validation still applies. Success is constrained to existing
response kinds: memory proposals stay pending/duplicate and checkpoints remain
provisional. No owner review, identity, delete, import, Provider send, native
unlock or arbitrary Workspace operation is exposed.

Native embedding code supplies a non-serialized Agent principal and a ceiling
of the five existing scopes. source:read is independent of memory:read. Claims
in clientInfo, tool arguments or imported material do not enroll or elevate a
caller. The typed AgentPort receives this native principal separately from the
validated request. It remains responsible for current record/project/sensitivity/
destination policy, native Host authority and durable principal-scoped receipts.
The grant constructor itself is **not** OS authentication.

Wrong response kind/requestId, malformed or oversized backend replies and
OutcomeUnknown disconnect without another dispatch. A nonretryable tool error
returns the original request packet/key for explicit recovery. The stdio
adapter never chooses new keys, reconnects, replays or logs payloads.

The explicit `mcp_synthetic_stdio --synthetic` example has **no Vault or Host
binding**. Its tools report backend_unavailable instead of fabricating durable
success. It is a protocol fixture, not a deployable Memory MCP server. This
slice does not map the current owner/window Host session to an Agent identity.

## Actual verification

| Check | Result |
| --- | --- |
| Eight new MCP boundary tests | Passed; fake-port contract calls, native principal/keys, scope isolation, owner/operation refusal, unknown reply, revocation, raw framing, stdout failure and request limits |
| `cargo test -p enouia-memory-host --locked -- --test-threads=4` | 35 passed, zero failures; existing native service/session/kernel/process transport cases retained; doctests zero |
| `cargo clippy -p enouia-memory-host --all-targets --locked -- -D warnings` | Passed with dependency closure |
| `cargo fmt --all -- --check` | Passed |
| Synthetic stdio example build | Passed, pinned Rust 1.98.1 GNU; no new external version |
| Separate-process stdio observation | Exit 0; five JSON-RPC replies for six input messages including initialized notification; protocol-only stdout and empty stderr; tools unavailable and unknown tools distinct; no-flag launch refused |
| Independent published tool-schema validation | python-jsonschema 4.26.0 validated all six self-contained Draft 2020-12 schemas and existing valid requests; every schema rejected a forged principal property |
| Independent existing schema cross-check | 43 schemas; record, set, IPC, Workspace, store, Host, Provider and extraction fixture checks passed |
| Runtime surface regeneration/check and owned-content review | Same 55 Workspace commands; aggregate recorded below; no machine schema or existing contract validator changed |

The first Host suite attempt under the restricted token passed the new MCP
tests but failed all four existing service tests while creating their synthetic
Vaults with AccessDenied. Running the same suite with normal permissions passed;
no ACL or product rule was weakened. The default Python lacked jsonschema; the
existing ignored venv already had the pinned 4.26.0, so no tool was installed.
Cargo.lock changes only the Host's direct edge to the already locked serde;
no package version or toolchain changed. Verification is scoped to Host and its
dependency closure; the full workspace suite/build was not rerun.

Saved synthetic process report and catalog are in ignored
`.local/mv82-mcp/process-report.json` and `catalog.json`. Fake success responses
come only from existing synthetic IPC fixtures; they do not establish real
commits, policy grants, quality, private-data access or a registered MCP client.

## Remaining exits and Runtime statement

The transport boundary and synthetic port are implemented. **Authenticated
scoped Host/domain binding is not implemented**, and neither are real-client
enrollment/compatibility or owner daily use. The next MV-8.2 slice must provide
an actual per-record-policy-enforcing AgentPort over the unique Host; it cannot
obtain access by relabeling the owner Workspace. Current owner native pipe
framing/compatibility and Runtime's embedded path remain unchanged.

The Runtime integration surface now also records the existing Agent IPC schema,
its validator and policy definitions because the new Host module depends on
them. Aggregate:
`0945be81a8c11a10ad6611d18be06fda55b2e872d827ccddc2aa159807e70d61`.
This is an additive Host library/build surface, not Runtime activation.
Runtime history-result adoption remains independently pinned to
`9d68d3d8707749ad3b0e2a36cfc97d71a27e712f` / **55 commands** /
`c88a12cce61b9601dffea3795a72decfe7de53082c0b4a10548947c993bd31e8`.
No Runtime checkout, Activity, real Provider, private Vault, multi-client
deployment, MCP registration, cloud, migration or manual status snapshot is used.

Primary protocol sources are linked in ADR-MEM-80; pinning this supported
revision is not a latest-version claim. Complete MV-8.2 and MV-8 acceptance
remain open, and no later MV stage starts from this report.
