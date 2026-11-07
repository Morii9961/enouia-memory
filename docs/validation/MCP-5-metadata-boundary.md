# MCP shared metadata boundary — synthetic evidence

2026-11-06. Continue codex/mcp5-opening-resources from f514016; draft #85 remains
stacked on unmerged MCP-4 #84. ADR126 / MCP_OPENING_RESOURCES_V1 clarified the
existing metadata shape before coding. The pinned official
[2025-11-25 RequestParams / notification definitions](https://github.com/modelcontextprotocol/specification/blob/main/schema/2025-11-25/schema.ts)
define _meta as optional object metadata. This slice checks its container only;
it adds no progress feature, field-level metadata policy or grant interpretation.

A failing synthetic test examined five non-object forms (null, array, boolean,
number, string) across initialize, initialized notification, tools/call,
tools/list and ping. Twenty paths incorrectly admitted metadata or advanced
initialization; the context catalog already refused its five cases
(target/mcp5-metadata-repro.log). Native authority still applied; this was
protocol/state conformity, not an authentication bypass.

ReadSession now computes metadata validity once. Invalid requests return
-32602/invalid_metadata after existing ID/session accounting; malformed
initialized notifications do not change phase. Valid absent/object metadata
is untouched and opaque. The check is shared by current read/report profiles;
the separate legacy MCP entry is unchanged. No capabilities, schemas, hello
fingerprints, record formats or valid tool meanings change.

Checks actually completed with pinned offline/locked Rust and synthetic roots /
owned hidden processes:

- opening_metadata 1 + opening_mcp 9 + opening_session_start 3 = 13 passed,
  0 failed (target/mcp5-metadata-fixed-tests.log). Checks include invalid setup
  not poisoning explicit valid setup, invalid notification not advancing phase,
  no read/cache changes, valid opaque metadata acceptance, and false Owner /
  SourceRead claims remaining permission_denied. Native snapshot is unchanged.
- opening_pipe: 10 passed, 0 failed (target/mcp5-metadata-pipe-tests.log).
  Five malformed actual read calls return invalid_metadata without a result;
  later valid calls and original recall stay usable. The unknown-publication
  case now runs prompt/tool with both object and null metadata. In all four
  scenarios native uncertainty wins before metadata parsing: no tool result,
  complete original Value retained, actual disconnect/revocation, no retry.
- Existing real pipe-6 save/replay case: 1 passed, 0 failed
  (target/mcp5-metadata-note-regression.log). hint_saved and independent writer
  authority retain their meanings. No NativeMemoryOwner test was executed.
- Runtime surface tests: 4 passed, 0 failed after recording the new compatibility
  entry (target/mcp5-metadata-surface-tests.log). An earlier in-progress run
  correctly failed the missing-log check; it is not claimed as a pass.
- Scoped Host lib / four affected test targets Clippy passed with warnings
  denied (target/mcp5-metadata-clippy.log). Formatting/surface/public checks
  passed. No full workspace run or dependency change.
- Python jsonschema 4.26.0: existing 77 schemas/corpora passed
  (target/mcp5-metadata-schema.log). This does not add a generic MCP request
  schema or claim independent schema validation of the new metadata state cases;
  those are behavioral tests against the pinned protocol definition. Optional
  actual-transcript checks were not enabled for this invocation.

An initial multi-target compile failed with No space left on device before the
behavior tests completed. Only four explicitly listed superseded, inactive
Cargo test executables inside this worktree's target/debug/deps were removed,
after path/reparse/data-marker/process/replacement checks (1,376,831,597 bytes).
No directory, .local material, data root or other worktree was removed. Retrying
with one compile job completed; the compile failure was not a product test
failure or successful validation. Prefer -j 1 while local build space is tight.

Current integration aggregate:
4cb740a0f0daab6363df2c07734a796b2c7e0e1a26b85979c45a0fb14ec8d11a.
The manifest and RUNTIME.md land with this correction. No Runtime pin/config,
private data, actual model/Provider, capture hook, forget-hide, Owner action,
mandatory queue, deployment, new MV or merge. Synthetic evidence does not prove
client/Runtime adoption, real model quality or owner acceptance.
