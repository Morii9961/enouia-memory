# MCP-3 checkpoint 2: Runtime-only local source transport

2026-10-27. Base 2e9c49c22b5ee42145bd2da485ba5e2023fcceeb. Branch
codex/mcp-runtime-capture-pipe. ADR-MEM-102 / RUNTIME_CAPTURE_PIPE_V1.

## Delivered boundary and matrix

Native RuntimeCaptureHost now binds once to a protected local pipe, borrowing
the same existing Vault and retaining native lifecycle control. No second
writer, Workspace forwarding, owner-control packet or public Agent write tool.
Native lock/revoke remains reachable through a retained Host Arc. Every service
exit revokes admission; repeated binding cannot reconnect. The existing strict
duplicate-key parser is shared internally, without changing old MCP behavior.

| Root/capability | This Runtime pipe | Agent surface |
| --- | --- | --- |
| schema 1 / host-current-v2 | Refuse | Existing six tools unchanged |
| schema 2 / automation-host-v2/v3 | Refuse | Existing versioned five-tool reads |
| schema 2 / automation-segmented-host-v1 | Explicit memory-runtime-capture-pipe-1 | Existing catalog reads, no capture grant |
| newer/unknown layout or claimed Agent/Owner/System producer | Refuse | No automatic upgrade |

The actual kernel process/SID/session, current exact policy/source pool,
expiry/revocation and instance gates precede source capture. The closed wire
cannot supply principal/scope/PID. Versions, schema digest, frame roles, byte
limits and 256-call ceiling are explicit. Source results stay truthful:
formation_scheduled=false, no job IDs and no formed-memory claim.

## Actual evidence

Five integration tests and three native transport unit cases cover an independent
owned Runtime process over actual local Windows pipes, valid preflight/capture,
same-key replay and changed-key-payload refusal; different actual kernel peer;
old Agent hello before any business transaction; native revoke/expiry during an
active connection; single-use binding; and legal witnessed capture followed by
an intentionally lost pipe reply. The latter asserts the live Vault remains
witnessed and unchanged after completion, client and Host retain original work,
and the client refuses another send without reconnecting. Child processes are
reaped. These counts include two child-process test entrypoints.

Contract/native cases also reject duplicate keys, wrong mode/major/capability and
claimed principal/scope/PID. Native closed receipt decoding checks exact request
hash, Vault, producer role, source count/uniqueness and preflight/capture flags.
Independent schema validation covers the actual preflight and successful capture
replies, plus a deliberately forged formation_scheduled=true response.

An initial transcript run used a relative output path while Cargo ran the test
from its crate directory; it failed with path-not-found and is not counted as
passing. The verified run uses an explicit workspace-owned ignored output path.
No product guard was relaxed. The earlier full-workspace run was interrupted
with STATUS_CONTROL_C_EXIT and is not counted as passing. The complete resumed
run on 2026-10-28 exited 0, including doctests: 744 passed, zero failed and one
existing ignored benchmark. Evidence is retained under ignored
target/mcp-segmented-validation/runtime-pipe-workspace-resumed.txt and its exit
file. Pinned offline/locked Rust 1.98.1, fmt and strict all-target workspace
clippy passed. Python 3.12 / python-jsonschema 4.26.0 independently validated
57 schemas, 12 native capture cases, nine wire cases, two actual Runtime pipe
exchanges and one invalid actual response. The unchanged catalog MCP transcript
also passed six actual exchanges and one invalid output against the updated
checker. Runtime surface generation/check and public diff checks passed;
aggregate 83ad27888db46956d64acb6a29ceaa783577fb4bfa5b5c94d54ce603b9f3c99d
is recorded in RUNTIME.md.

## Requirements 03 differences and next work

Morii selected Runtime hooks only, superseding Agent-host capture. This is the
Memory-side local source entry, not a Runtime repository hook implementation
or conversation_capture/live-job advertisement. The initialization session owns
its shared intake/formation entry; this slice does not modify that worktree or
create a second formation pipeline. Adopt its published exact contract before
connecting durable ordered live jobs. Agent memory_note, normalized quotation,
checkpoint-on-new-format, reports/forget, session_start and resource links remain
pending. Newer owner layouts/projections still require separate reader adoption.

No real Provider/embedding, private Vault/export/credentials, real client config,
Activity/Usage input, remote listener, Runtime edit/pin, deployment or main merge.
Only new synthetic roots, bounded native metadata and owned test processes.

Owned files: Runtime capture transport/unit tests and real-process integration
test; its schema/corpus/checker and contract test; minimal producer single-use
binding/getter and internal JSON visibility; ADR102/contract/report and Runtime
manifest/log. No AGENTS, manual state or parallel initialization/owner/projection
source files were changed.
