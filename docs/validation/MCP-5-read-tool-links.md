# MCP-5 read tool resource links — synthetic evidence

2026-11-06. Continue codex/mcp5-opening-resources from dab0160, draft PR #85
stacked on unmerged MCP-4 PR #84. ADR126 / MCP_OPENING_RESOURCES_V1 were frozen
before coding this slice. No new MV, real client, model, root or Runtime adoption.

Only the context profile appends a standard resource_link after the original
text for successful non-null memory_read / memory_recall results. Structured
native data and its text copy are unchanged. The memory URI derives from the
returned authorized item; the recall URI derives from its returned request ID.
Names/mime metadata are static. Missing data, known errors and other read tools
produce no link. Old initialized/notes/report profiles keep their original
tool-result content. No extra native read, selection or automatic fetch occurs.

The memory link uses the existing current scoped resource recipe: independent
current authorization, no source text, default first history page and 65536-byte
budget. It does not reproduce a tool's source-text/page/expected-snapshot choice.
Recall links return only the same reader's actual current cache, including a
successful bounded empty selection. Failure does not create or overwrite cache.
No link grants SourceRead, Owner consent, evidence status or write permission.

New mcp-context-tool-links-v1 / linked_result_schema(tool) freezes the complete
memory_read and memory_recall JSON-RPC result envelopes, including missing/error
native results without links. Both schemas enter pipe-10's exact fingerprint.
An actual-pipe test rebuilds the hash and proves omitting these schemas changes
it. Matching endpoints are required; no fallback, reconnect or replay.

Checks actually run with pinned offline/locked Rust and new synthetic roots /
owned hidden processes:

- opening_pipe: 10 passed, 0 failed (target/mcp5-tool-links-tests.log). Actual
  read/recall links resolve through the existing native resources. Five captured
  envelopes cover read present/absent/error and recall present/error. Other three
  tools keep one text item, native data remains exact and snapshot is unchanged.
- opening_mcp 9 + opening_session_start 3 + opening_tool_links 3 = 15 passed,
  0 failed (target/mcp5-tool-links-direct-tests.log). Together these current four
  targets total 25 passing tests, run in two groups; the earlier two-test version
  of opening_tool_links is not counted again. New cases cover independent
  MemoryRead without SourceRead, refused source-text reads, no links on null /
  error results, bounded empty recall, current resource versus tool source text,
  unchanged native values/cache/snapshot and legacy text-only results.
- Contract mcp_opening 5 + runtime_surface 4 = 9 passed, 0 failed
  (target/mcp5-tool-links-contract-tests.log).
- Independent Python jsonschema 4.26.0: 77 schemas OK, 23 new linked-read cases
  and 5 actual envelopes checked against both method-specific source schemas
  and exported bundles (target/mcp5-tool-links-schema.log). Parsed text equality
  and URI/returned-native-identifier equality are checked separately. Well-formed
  mismatched text/IDs illustrate that JSON Schema alone cannot enforce them.
  Optional older prompt/session transcript checks were not enabled in this run.
- Scoped Host lib / four affected test targets and Contract two affected test
  targets passed Clippy with warnings denied. Formatting, Runtime surface and
  public-content checks passed. No full-workspace or NativeMemoryOwner test ran.

Current integration aggregate:
20fcbe005023e0070acd996c5e72436809d806858ec815287d02e1f1793687f6.
The new schema, manifest and RUNTIME.md acknowledgment land with implementation.
Raw actual transcripts/logs remain ignored; the committed corpus is synthetic.

No session creation, capture hook, model dispatch, formation, reinforcement,
Owner action, queue, forget-hide, Runtime edit/pin/config, deployment or merge.
All existing uncertainty/disconnection/original-key recovery rules remain.
These are synthetic protocol/domain tests, not real model quality, private-data
validation, Runtime product use or owner acceptance.

## Same-ID current revision versus stale recall (2026-11-06)

After b9bc1c9, added one focused lifecycle case without implementation/schema /
surface changes. The read-only context server first returns a memory link and
an actual-recall link. A separate synthetic System operator updates the same
memory ID using the already persisted Runtime UserStatement source from the
capture fixture. Input content and quoted evidence are that captured raw text;
no Agent hint or new synthetic source is substituted, and NativeMemoryOwner /
fixture.revise is not called. No actual model or Runtime product is used.

The old recall URI returns stale_scope. The original memory URI still resolves
through fresh qualified detail to revision 2 with the exact captured user text,
source_read=false and a new snapshot. Reading that memory URI does not refresh
the stale recall cache. Only an explicit new recall produces the current
revision with the same scope hash and replaces the old recall entry. Reads keep
the post-update snapshot unchanged and create no captures or Owner actions.

The single lifecycle test passed, 0 failed
(target/mcp5-current-memory-link-revision.log). Scoped opening_tool_links Clippy
and formatting passed. Earlier schema/surface evidence remains applicable; these
checks were not rerun for this test-only slice. No full-suite or real-use claim.
