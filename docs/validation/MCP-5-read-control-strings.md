# MCP read control strings — synthetic contract correction

2026-11-05. Continue codex/mcp5-opening-resources from 2fde3f7, draft PR #85
stacked on unmerged MCP-4 PR #84. No merge, real client or new MV is activated.
ADR126 / MCP_OPENING_RESOURCES_V1 were clarified before the correction.

The frozen read schemas require operation and recall intent to be strings.
The MCP adapters used serde unit enums, which also accepted externally tagged
objects such as operation={memory_search:null} and intent={browse:null}.
A targeted failing test reproduced all five operation and all three intent
forms on both context and initialized entry points: 16 malformed admissions
(target/mcp5-read-string-enum-repro.log). These were malformed reads, not new
permissions or writes; native authority still applied.

The minimal correction uses one reusable string_enum deserializer at those
owned control-field boundaries in initialized and v2 adapters. It requires a
String, then deserializes the existing typed enum. No repeated raw-value guards,
new dependency, schema/version/capability, output change or compatibility hash
change. Valid pipe-5/6 meanings remain unchanged. The two existing fixture
manifests add eight negative cases each; independent schemas already reject them.

Checks actually run after correction, pinned Rust/offline/locked and new
synthetic roots/owned processes only:

- opening_mcp 9 + opening_session_start 3 passed, 0 failed. The new case checks
  content-free invalid-argument replies, zero initialized dispatch counts,
  preservation of the previous actual context and unchanged native snapshot
  (target/mcp5-read-string-enum-fixed-tests.log).
- Actual context pipe cross-entry case: 1 passed, 0 failed. Eight malformed
  controls refuse; the original recall remains available; valid five-tool reads
  and the prompt keep their exact native context and ephemeral cache semantics
  (target/mcp-read-enum-context-pipe-tests.log).
- Actual v2 pipe malformed-input/refused-old-client case: 1 passed, 0 failed;
  the eight new cases reject, then the connection retains its original source
  permission refusal (target/mcp-read-enum-v2-pipe-tests.log).
- Actual v2 all-five-tools success/trace/no-reinforcement case: 1 passed,
  0 failed (target/mcp-read-enum-v2-success-tests.log). Only the two named
  automatic_loop cases ran; no NativeMemoryOwner test or real model was invoked.
- Runtime surface tests: 4 passed, 0 failed
  (target/mcp-read-enum-surface-tests.log).
- Python jsonschema 4.26.0: 76 schemas OK, including agent_read_documents=18 and
  agent_initialized_documents=22 with the new negative cases. This invocation
  did not enable optional actual-transcript validation
  (target/mcp-read-enum-schema.log).
- Scoped Host lib / affected test targets Clippy with warnings denied, fmt,
  Runtime surface and public-content checks passed. No full-workspace run.

Current integration aggregate:
0a969a802b2d919f209cb976662b2e9f3112d689fbd1ad5aaad9d5417f2a0c76.
The manifest and RUNTIME.md are updated in the same implementation commit.

Continue checking the same control fields in the adjacent catalog/qualified
adapters before treating this class as closed. No domain-wide enum rewrite,
new runtime validator or speculative authority mechanism is supplied. All
existing capture/evidence/unknown-result/private-data/Runtime boundaries remain;
targeted synthetic evidence does not prove real adoption or owner acceptance.
