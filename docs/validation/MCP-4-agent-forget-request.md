# Request-only Agent forget recording

2026-11-04. Base `13c57de`. [ADR125](../adr/125-agent-forget-request-recording.md)
and [contract](../contracts/AGENT_FORGET_REQUEST_V1.md) frozen before coding.
Implementation and focused checks are complete.

Independent AgentForgetPermit/Requester records an untrusted request beside an
authorized exact target and original Runtime UserStatement quote. The existing
Owner chain gains only agent_forget_request. Its independent receipt says
forget_request_saved. No source, memory revision, recall hiding/suppression,
Owner action, model dispatch, worker change or human review queue is created.
Scope, duration, restoration and revocation rules for forget-hide remain unchosen.
No public draft/prepare feature was added; submission accepts original requests.

Explicit pipe-9/MCP/stdio pair the same Agent/exact leased Vault's independent
reader and requester. Unknown publication emits no tool result/Reply, disconnects
and revokes both grants, retaining the full original parsed MCP Value. Original
keys recover only after explicit native recovery/fresh enrollment.

Owned files: forget contract types, native/MCP schemas/corpora/tests, contract
module/delta/catalog schema; Vault agent/intake/state/process/export; Host
requester/MCP/pipe/export/launcher and native/server/pipe tests/shared fixture;
ADR125/register, contract, this report, continuation handoff, Python cross-check
and Runtime log/surface. No worker, AGENTS, manual status or Runtime repo edits.

## Checks on the final source

- Pinned Rust 1.98.1, offline/locked, serial Windows tests:
  `cargo test -p enouia-memory-host --test agent_forget --test agent_forget_pipe --test agent_correction --test agent_supersede --test initialized_mcp --locked --no-fail-fast`:
  **52 passed, 0 failed** (9 forget native/server, 7 forget pipe/stdio,
  13 correction, 15 supersede, 8 initialized read).
  Log: `target/mcp4-forget-host-tests.log`.
- Three contract targets (agent_forget, agent_supersede, agent_correction):
  **6 passed, 0 failed**. Log: `target/mcp4-forget-contract-tests.log`.
- Python jsonschema 4.26.0: **72 schemas OK**, including 22 stored forget
  cases/segment and 24 MCP cases. Hidden/deleted/forgotten/queued success claims
  are rejected. No dependency/toolchain changes or installation.
- Strict Clippy for Host library/binaries and the five selected tests passed:
  `target/mcp4-forget-clippy.log`. Changed Vault library strict Clippy also
  passed; log: `target/mcp4-forget-vault-clippy.log`.
- `cargo fmt --all -- --check`, Runtime surface and `git diff --check`: passed.
  Aggregate: `222a23794af63f4a1abd84abcb1ca8038ef47dd746b48220427a3e58028237d1`.
- No full-workspace rerun after this request-only addition. The prior full
  948 passed / 0 failed / 4 ignored and workspace/all-target Clippy belong to
  `13c57de`; that worker implementation is unchanged. These focused checks
  cover the new intake/state/transport and adjacent reports/read fingerprint.

An initial compile caught obsolete draft-only constructor fields after removing
public preparation, and copied test imports/constant names. They were corrected
before the passing run; no compatibility wrapper or dependency was added.

## Coverage

Saving leaves the exact target/hash, sources and Owner state unchanged, with no
captured work or journal jobs. Explicitly stepping capture, correction and
supersede consumers dispatches none of them. Request keys are separate from
correction/supersede keys for the same Agent/request ID/target. Tests cover exact
Runtime UserStatement and target/source closure, missing source-read authority,
wrong Owner, blank/oversized input, stale target/source, lock/revoke/expiry/policy,
authorization before receipt replay, replay after System update/reopen, conflict,
capacity and concurrent replay. Server tests reject foreign Agent/Vault and
forged packets and advertise closed local-only schemas. Actual owned Agent/pipe
and stdio tests cover old profiles, wrong peer/scopes/hash, lifecycle invalidation,
unknown publication before/after CURRENT, actual disconnect, full original MCP
retention, blocked changed sends, explicit original-key recovery, stdout failure
and the real launcher. There is no forget analyzer or effect executor.

Only synthetic roots, scripted analyzers and owned processes. No private root,
real model/Provider/embedding, client configuration, egress or Runtime adoption.
Synthetic tests do not establish product use or Owner acceptance. The three
authorized MCP-4 route items stop after request-only forget; MCP-5, hide/delete,
new MV stages and merging remain separate instructions.
