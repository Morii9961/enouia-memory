# MCP-5 complete response contracts — synthetic evidence

2026-11-05. Based on f425c17, codex/mcp5-opening-resources / draft PR #85
(base: unmerged MCP-4 PR #84). Morii explicitly authorized continued MCP-5
synthetic development. ADR126 and MCP_OPENING_RESOURCES_V1 were amended before
implementation. Scope is the five existing context methods' response contracts.

## Change and compatibility

Existing mcp-opening-v1 / schema(startup, output) cover arguments and parsed
memory content. New mcp-opening-wire-v1 / response_schema(method) cover complete
JSON-RPC successes for resources/list, resources/templates/list, resources/read,
prompts/list and prompts/get. Closed catalog/text shapes follow the pinned
official MCP 2025-11-25 resources/prompts signatures linked in ADR126; they
describe this profile's subset, not every MCP extension. No business method,
native permission, source capture, worker, model call or error behavior changes.

The five method-specific schemas enter the exact pipe-10 compatibility hash.
Actual advertised catalogs/guide/preamble reconstruct that hash; removing the
response schemas produces the prior different fingerprint. Mismatch still
disconnects before business calls and never falls back or reconnects. No older
binary was run on any root. Existing pipe-5 and report profiles are unchanged.

## Checks actually run

Pinned Rust toolchain, locked/offline, incremental disabled, one test thread,
new synthetic roots and owned Windows processes only. No private data/model.

- opening_mcp: **8 passed / 0 failed**. Source of the read behaviors is unchanged
  after this run. Log target/mcp5-wire-tests.log also records the initial pipe
  evidence-output path failure below; it is not a wholly passing run.
- opening_pipe: **6 passed / 0 failed**, target/mcp5-wire-pipe-tests.log. After
  adding response-ID/request-URI/inner-URI correlations, its affected actual-pipe
  test reran: **1 passed**, 5 filtered, target/mcp5-wire-correlation-tests.log.
- Contract mcp_opening: **2 passed / 0 failed**, existing payload corpus plus
  **31 wire cases**. Refuse owner/extra fields, simultaneous error+result,
  invalid success IDs, role/content changes, binary or multiple contents,
  unsupported URI/templates, cursors and altered required prompt arguments.
- Python jsonschema 4.26.0 independently validates **74 schemas**, all existing
  corpora, the 31 new cases, **7 actual success replies** against both document
  and method-specific bundled schemas, and **4 separately parsed payloads**.
  Log target/mcp5-wire-schema.log; the synthetic-only transcript is generated at
  target/mcp5-wire-transcript.json through ENOUIA_MCP_OPENING_TRANSCRIPT. Use an
  explicit absolute owned path because Cargo's test cwd is the crate directory.
- Scoped strict Clippy: Host lib, opening_pipe and opening_mcp passed;
  target/mcp5-wire-clippy.log. Final fmt/whitespace, public-content review
  (14 owned files; no private-path/credential-pattern matches) and integration
  consistency passed.

The first pipe invocation passed its response/payload assertions but failed to
write the optional transcript: its relative path targeted a missing crate-local
target directory. Python then had no transcript to read. Correcting the test
invocation to an absolute existing owned target path passed; no product storage
workaround, path guessing or private root access was added.

Surface aggregate:
fadc0afc1991275de9972277e7ab98d16a0c4a789c31c9a424e82e773dd5222d.
No full workspace rerun: localized contract/handshake changes, no dependency or
storage change. Previous 913/948 totals do not describe this HEAD. Synthetic
contract/own-process evidence is not real model quality, Runtime adoption or
Owner acceptance. No real client configuration, Runtime repository/pin/Activity,
forget-hide, external Agent capture hook, deployment, automation, manual project
status snapshot, main merge or new MV stage is included.

## Follow-up: native identity/profile matrix

Based on pushed f731161. Test/documentation-only clarification; production code,
schemas, validator and integration surface unchanged. The pipe matrix now covers
**15 refused combinations**, including all adjacent pipe-5 through pipe-9,
wrong Vault ID, wrong native Agent, SourceRead-only / ContextRead-only without
MemoryRead, wrong hash/claimed Owner, missing SourceRead for a source-enabled
reader, extra write/context scopes and wrong kernel peer. Each refusal revokes
the native reader and leaves the synthetic snapshot unchanged. These are
current binary protocol-profile checks, not executions of old binaries.

The source-disabled reader succeeds with either MemoryRead-only enrollment or
MemoryRead+SourceRead enrollment; both return source_read=false / sources=[].
Native delegation remains the authority ceiling. The contract's earlier
"exactly when" wording is clarified to avoid implying that kernel registration
can expand that native grant. No permission behavior changed.

opening_pipe **6 passed / 0 failed**, target/mcp5-identity-matrix-tests.log. The
17 matrix configurations above are cases within those tests, not 17 tests.
No Python/full suite/surface regeneration: their relevant source is unchanged.
Scoped Clippy, fmt/whitespace and public-content checks passed (four owned files,
no private-path/credential-pattern matches).

## Follow-up: current formation and explicit fresh reads

Based on 1d01558; test/contract clarification only. One owned real pipe keeps
the same enrolled reader/connection while an independently delegated optional
AgentNote is saved and a second memory is formed by the existing System
evaluator from the already-authorized synthetic Runtime UserStatement. No
analyzer, model, new capture authority or report consumer runs.

The old recall URI returns content-free known stale_scope and does not rerank.
The caller then explicitly sends a new memory_start read with its own read ID
on that still-authorized connection. It returns two current memories with the
same scope hash/new snapshot, no Agent note marker, and exactly the response
later returned through the new recall URI. The old URI is not an alias for the
new context. Read-only steps leave the post-formation snapshot unchanged; EOF
revokes the reader, no capture jobs or Owner actions are created. This is a
known stale read, not OutcomeUnknown recovery, replay or automatic reconnect.

The affected test **1 passed / 0 failed**, six filtered;
target/mcp5-stale-context-tests.log. Production/schema/validator source unchanged,
so prior targeted/schema/integration evidence applies. Scoped Clippy and
fmt/whitespace/public-content review passed (four owned files; no matches). These
synthetic steps do not prove real Runtime worker/model adoption or capture of
new sources outside the original delegated source closure.

## Follow-up: method-specific inner outputs

Based on f7c92d5. Production code, JSON schemas and the integration fingerprint
are unchanged; test tooling now selects the output definition for the actual
method. The combined mcp-opening-v1 document also accepts valid requests, so a
whole-document check alone is insufficient for an embedded output slot.

The corpus grows to **36 outer-envelope cases**, including **5 typed inner
cases**. A resource request is accepted by the combined document but refused
as resource output. A guide is valid resource output and valid in the combined
document, but refused as opening recall. An Agent-claim report_saved receipt
under an otherwise valid MCP user/text message and malformed embedded JSON are
also refused as outputs. A user role or valid envelope does not authenticate
content. This is validation coverage, not a production exploit claim.

The actual-pipe fixture exports method-tagged payloads and both output bundles.
Independent Python validates the 4 actual payloads against the method-selected
source definition **and** compiled output bundle, and tests the parseable inner
counterexamples against those bundles. All 7 outer replies still validate.
The optional synthetic transcript must be regenerated for this tooling format;
no compatibility fallback or production persistence is added.

- Affected actual-pipe test: **1 passed**, six filtered;
  target/mcp5-typed-payload-tests.log.
- Contract mcp_opening: **2 passed / 0 failed**. Rust also verifies the combined
  root's acceptance of the request/guide counterexamples.
- Python jsonschema: **74 schemas / 36 outer cases / 5 typed inner cases /
  7 actual replies / 4 actual payloads**, all registered corpora agree;
  target/mcp5-typed-payload-schema.log. No tools/dependencies installed.
- Scoped opening_pipe strict Clippy, final fmt/whitespace and public-content
  review passed (seven owned files; no matches). No full suite or surface regeneration:
  relevant production source is unchanged from the earlier verified revision.
