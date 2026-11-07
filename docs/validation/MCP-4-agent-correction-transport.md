# Explicit Agent correction MCP and pipe transport

2026-11-04. Base `2a6310e` (PR #83 includes `f42f3b1`).
[ADR123](../adr/123-agent-correction-mcp-transport.md) and its MCP/pipe contracts
were frozen before implementation.

The opt-in six-tool server and distinct pipe-7 pair independent native read and
correction delegations on the same leased Vault/Agent. Existing notes/read
profiles retain their meanings. Success means only report_saved. Native unknown
publication produces no tool result/Reply and actually disconnects, revoking
both grants; Client/stdio retain the full original parsed MCP Value and block
sends. Explicit original-key recovery is the only replay route after uncertainty.

Owned files: ADR123/register, MCP/pipe contracts, new wire schema/corpus,
contract test and Python cross-check; Host correction server/native transport,
MCP export, reporter live gate, launcher, correction/server/pipe tests and shared
synthetic correction fixture; Vault correction live gate; this report,
Runtime adoption log and generated surface. Existing native fixture helpers were
moved for reuse without changing their logic. AGENTS/manual status are unchanged.

## Checks

- Pinned Rust 1.98.1, offline/locked, serial Windows tests:
  `cargo test -p enouia-memory-host --test agent_correction --test agent_correction_pipe --test agent_note --test agent_note_pipe --test initialized_mcp --locked --no-fail-fast`:
  **44 passed, 0 failed** (13 correction/server, 7 correction pipe/stdio,
  9 note/server, 7 note pipe/stdio, 8 initialized pipe).
  Log: `target/mcp4-correction-transport-tests.log`.
- `cargo test -p enouia-memory-contract --test agent_correction --locked`:
  **2 passed**, including the 18-case correction MCP corpus.
  Log: `target/mcp4-correction-contract-tests.log`.
- Python jsonschema 4.26.0 cross-check in the existing venv: **68 schemas OK**,
  including the 18 correction MCP cases. No dependencies installed.
- Strict Clippy for the Host library, binaries and the five selected test targets:
  passed (`--locked -- -D warnings`). Log:
  `target/mcp4-correction-transport-clippy.log`.
- `cargo fmt --all -- --check`, `python tools/integration/runtime_surface.py`
  and `git diff --check`: passed. Surface:
  `1f3c9f79adbf180b494e0f5fa4a4c7af13cb86ad422b79706f26a21d741336c8`.
- No full workspace test run. The prior 913-test result is historical evidence,
  not a result for this change.

An initial test compile failed because moving the shared fixture also removed
an import still used by one existing test. The import was restored before the
successful focused run. No product workaround was added.

## Coverage and continuation

The server refuses a different Agent and a different native Vault instance with
the same logical ID. It advertises local-only schemas, forwards real reads,
rejects authority/version fields and duplicate MCP IDs, returns bounded known
refusals, and replays the original receipt after a lawful System revision.
Before/after-CURRENT faults retain the complete original Value without a tool
result. Owned Agent processes exercise actual disconnect, old pipe-5/6 and
forged/hash/scope/peer refusal, revoke/lock/expiry/policy/enrollment invalidation,
original-key recovery, stdio output failure and the real launcher. A saved
report then forms one user-quote-backed System correction with no Owner queue.

Next authorized route: independently scoped memory_supersede_report (previously
true, now changed; System update rather than correction), then request-only
memory_forget_request. Freeze each slice's contracts before coding, validate and
commit/push separately, and update the draft PR. Do not merge without Morii.

Only new synthetic roots and owned child processes are used. No real Vault,
model/Provider/embedding, private data, client configuration, Activity/Usage or
Runtime repository is opened. Synthetic evidence is not Runtime adoption or
Owner acceptance. Supersede and request-only forget follow in separate slices;
forget-hide, MCP-5 and new MV stages remain outside scope.
