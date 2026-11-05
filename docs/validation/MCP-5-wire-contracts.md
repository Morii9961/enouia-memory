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
