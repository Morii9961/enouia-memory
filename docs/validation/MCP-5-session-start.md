# MCP-5 session_start — synthetic evidence

2026-11-05. Branch codex/mcp5-opening-resources, implementation based on
399a217; draft PR #85 remains stacked on unmerged MCP-4 PR #84. Morii asked to
continue the authorized roadmap, retaining all existing hard boundaries.

ADR127 and AGENT_SESSION_START_MCP_V1 were frozen before implementation. The
pipe-10 six-tool catalog now exposes an Agent-invocable opening recipe instead
of requiring a prompt/manual step. session_start uses the same qualified reader
and bounded native recall as memory_start, returns original request_id and the
exact recall resource payload, and creates no durable Session or capture. No
model, Provider, Owner operation, checkpoint, job or mandatory queue is added.
Guide discovery and exact hello fingerprint bind the tool descriptor/schemas;
pipe-5/6 and report profiles retain their meanings.

Checks actually run with the pinned toolchain, offline dependencies and new
synthetic roots/owned hidden child processes:

- Host opening_mcp: 8 passed; opening_session_start: 3 passed. Original run
  target/mcp5-session-start-tests.log also ran the then-current 9 pipe tests.
- After adding the actual session tool pipe case and extending the existing
  unknown-publication case to both prompt/tool, opening_pipe: 10 passed, 0 failed
  (target/mcp5-session-pipe-tests.log). Together the current host targets contain
  21 passing tests; the old 9-test pipe run is not counted again.
- Contract mcp_opening: 3 passed; runtime_surface: 4 passed, 0 failed
  (target/mcp5-session-contract-tests.log).
- Python jsonschema 4.26.0: 75 schemas; 18 new session input/output mutation
  cases and 2 actual session payloads validated against both source and exported
  schemas. Existing 36 wire cases, 5 typed-inner cases, 7 actual envelopes and
  4 actual resource/prompt payloads also passed. Full cross-check: OK
  (target/mcp5-session-schema.log).
- Scoped Host lib/three affected test targets and Contract two affected test
  targets passed Clippy with warnings denied; cargo fmt --all -- --check passed.
- Runtime surface regenerated and checked, compatibility aggregate recorded in
  RUNTIME.md: 05a92cd3e3cc0889f84e68fcec016617f75ced5f50b227b8982d895a0cde2fde.

The first fixture-generation attempt used Windows' default text encoding and
failed before writing the corpus. Explicit UTF-8 corrected the harness; the
subsequent cross-check above passed. No product behavior was inferred from that
failed attempt. Transcripts use absolute output paths because Cargo's test cwd
is the package directory; ignored raw transcripts/logs are not public artifacts.
The committed corpus is synthetic and includes no local paths or credentials.

Evidence covers closed inputs, independent MemoryRead with optional SourceRead,
advertised schemas/annotations, actual context/resource equality, bounded
output, invalid input preserving the previous cache, unchanged native snapshot,
no queued captures/Owner actions, and old initialized profile rejecting this
tool. OutcomeUnknown at the native live gate produces no result, retains the
complete first Value including original RPC ID/_meta, disconnects and revokes;
there is no automatic retry, reconnection or replacement key.

No full workspace run was needed for this localized service/schema change;
earlier 913/948 baselines are not evidence for this HEAD. No real model/private
Vault/Runtime adoption, product use, deployment or owner acceptance is claimed.
Merging remains a separate owner instruction. Manual project snapshots remain
untouched. Continue only MCP-5 synthetic opening/resources work; no new MV,
forget-hide, external Agent capture hook or real client configuration.

## Standard resource links follow-up

Based on bf4a747. ADR127/contract were extended before coding. Successful
session_start content now has its existing text copy followed by one standard
resource_link to the exact actual recall URI; static name/mime metadata reveals
no memory title. Structured output stays unchanged. The closed complete success
envelope is versioned separately as mcp-session-start-wire-v1 and exported by
session_result_schema(). The native exact fingerprint binds this schema; an
actual pipe test reconstructs the hash and confirms removing this contract
changes it. No old tools/profile, native authority or persistent format changes.

Checks actually run after this change:

- opening_pipe 10 + opening_session_start 3 = 13 passed, 0 failed
  (target/mcp5-resource-links-tests.log). The subsequent fingerprint assertion
  was then run as one focused passing test, not counted as a new unique test
  (target/mcp5-resource-link-fingerprint-tests.log).
- mcp_opening 4 + runtime_surface 4 = 8 passed, 0 failed
  (target/mcp5-resource-links-contract-tests.log).
- Independent Python: 76 schemas, 16 new link-envelope cases, both actual
  session payloads and the actual complete link envelope passed source and
  exported schemas, alongside existing opening cases/transcripts
  (target/mcp5-resource-links-schema.log).
- Scoped Host lib/affected tests Clippy, formatting and Runtime surface checks
  passed. Aggregate: ef4679f92dc027f24d64378fa43b80d3756aec354f81ef3fa1f1e8df8d512320.

Tests distinguish structural schema checks from cross-field checks: a
well-formed but different recall URI or mismatched text copy can pass JSON
Schema, yet fails the separate equality checks. The actual pipe response
preserves original RPC ID, links its own context URI, and resources/read returns
that exact native context. A link is neither a grant nor an automatic fetch;
normal scope/snapshot/cache gates, unknown disconnection and explicit recovery
rules remain in force. Synthetic evidence only; no real client/model or
Runtime adoption is claimed, and no full workspace run is implied.
