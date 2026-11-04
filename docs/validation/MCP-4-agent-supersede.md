# Independent Agent supersede reports

2026-11-04. Base `07626b4`. [ADR124](../adr/124-agent-supersede-reports.md)
and [contract](../contracts/AGENT_SUPERSEDE_V1.md) frozen before coding.

Independent native delegation and agent_supersede delta save an untrusted
interpretation beside real captured Runtime UserStatement evidence. The
supersede_report_saved receipt and key space do not borrow correction authority.
The shared leased worker gains a separate SupersedeAnalyzer and optional
supersede_key journal field. Only change=update can reach the existing evaluator;
capture/correction/supersede dispatch cannot overlap. Existing worker entries
retain their consumer set; start_worker_with_reports explicitly opts into all.
Distinct MCP server/pipe-8 and launcher attach mode preserve full unknown packets
and actually disconnect before Reply. Older binaries cannot open new deltas.

Owned files: new supersede contract types, native/MCP schemas and corpora/tests;
contract module/delta/catalog schema; Vault agent gate, supersede intake,
state/replay/process/export and formation worker; Host reporter, captured worker,
supersede MCP/pipe/export/launcher, native/server/pipe tests/shared fixture and
Owner worker test; ADR124/register, contract, this report, continuation handoff,
Python cross-check and Runtime log/surface. No AGENTS or manual status edits.

## Checks on the final source

- Pinned Rust 1.98.1, offline/locked, serial Windows tests:
  `cargo test --workspace --locked --no-fail-fast`: **948 passed, 0 failed,
  4 ignored across 114 targets**, including all doc-tests, exit 0.
  Log: `target/mcp4-supersede-workspace-tests.log`. No Rust source changed after
  the run started. This is new evidence after ADR123/124, not the old 913 result.
- First focused Host run: **59 passed** (13 correction, 7 correction pipe,
  14 supersede, 7 supersede pipe, 18 Owner capture/worker). The final full run
  includes 15 supersede tests after adding pending dispatch exclusion.
  Log: `target/mcp4-supersede-host-tests.log`.
- Python jsonschema 4.26.0: **70 schemas OK**, including 22 stored supersede
  cases/segment and 19 MCP cases. Both Rust supersede corpus tests passed in
  the full run. No dependencies installed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
  Log: `target/mcp4-supersede-workspace-clippy.log`, with the successful Finished
  marker. The execution session later expired after completion; neither tests
  nor Clippy needed another run.
- `cargo check -p enouia-memory-host --locked`, `cargo fmt --all -- --check`,
  Runtime surface and whitespace checks: passed.
- Runtime aggregate:
  `ec66818e22d355d386976534d0a1d3286a251bf8f5cc8340da1617deb16728db`.

## Coverage

Independent correction/supersede grants, keys and receipt states with the same
Agent/request ID/target; exact Runtime user quote/role and target closure;
generation/policy/expiry/deletion barriers; original-key replay across a System
revision and chain reopen; conflict/capacity/concurrent replay; full-value
unknown publication before/after CURRENT and explicit recovery. System analysis
accepts only one resolvable update statement, refuses correction/invalid quotes,
never resends unknown or restarted analysis, refuses moved/deleted inputs and
keeps revision history. Pending supersede blocks capture/correction dispatch.
The actual hosted worker and owned Agent/stdio processes exercise the automatic
update, separate pipe-8/old-profile refusal, paired authority, lifecycle stops,
real disconnect, full original retention, stdout failure and launcher mode.

The previously noted old continuation revocation timing test passed in this
full run. Its source is unchanged; this does not prove that race is fixed.

Only synthetic roots, scripted analyzers and owned child processes. No real
model, private root, client configuration, Runtime edit/pin, egress or Owner
acceptance. Next authorized route remains request-only memory_forget_request;
forget-hide, MCP-5, deployment and merging remain separate authorization.
