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

## Catalog / qualified follow-up (2026-11-06)

Continue from c346838 with the same frozen string-only contracts. The adjacent
catalog and qualified adapters each reproduced all eight malformed tags before
correction (target/mcp-read-enum-catalog-repro.log and
target/mcp-read-enum-qualified-repro.log). Each now reuses string_enum on
operation and intent: four field annotations, no domain-wide enum rewrite,
new capability, output change or fingerprint change.

Checks actually completed:

- Catalog direct control-field and qualified pre-dispatch tests: 2 passed,
  0 failed. Eight cases each now reject with zero native dispatch counts;
  the catalog snapshot is unchanged (target/mcp-read-enum-catalog-fixed.log,
  target/mcp-read-enum-qualified-fixed.log).
- Actual executable stdio for each profile: 2 passed, 0 failed. Eight malformed
  calls reject, then the original valid recall still returns its exact native
  trace without modifying business bytes; EOF reaps the owned process
  (target/mcp-read-enum-catalog-stdio.log,
  target/mcp-read-enum-qualified-stdio.log).
- Independent Python: 76 schemas OK; catalog and qualified manifests each
  cover 22 documents, including eight new negative controls. Optional actual
  transcript validation was not enabled (target/mcp-read-enum-adjacent-schema.log).
- Scoped Host lib / qualified_mcp / segmented_agent Clippy passed with warnings
  denied. Runtime surface tests: 4 passed, 0 failed
  (target/mcp-read-enum-adjacent-clippy.log,
  target/mcp-read-enum-adjacent-surface.log). Formatting/surface/public checks
  passed. No full workspace or NativeMemoryOwner test was run.

The initial attempt to run Clippy/surface tests was not executed because the
automatic approval review hit the usage limit; it was not judged unsafe. After
Morii's continuation, that same authorized check completed on 2026-11-06.
Earlier successful behavior/schema evidence was reused because its source was
unchanged. No retry of a Memory operation or unknown publication was involved.

Current integration aggregate:
2c856f8deb96d9dd8bafc68ee4a8f97bd6e66f2e28808aa11cb56af768a38520.
This closes the adapter-owned operation/intent representation gap in the four
read adapters. It does not claim domain-wide enum/schema equivalence, real
client use, Runtime adoption, private-root validation or owner acceptance.

## Catalog filter string enums (2026-11-06)

Continue from 7f98d2b. The existing catalog-v2 filter allows nullable strings for
form/state/knowledge/adoption/validity/attention/time_axis. Raw objects with a
unit tag were instead normalized by Core serde parsing and dispatched. Before
the correction, the focused context/initialized case recorded 14 admissions,
and the catalog/qualified cases recorded seven each: 28 malformed paths
(target/mcp5-filter-shape-repro.log, mcp5-filter-catalog-repro.log,
mcp5-filter-qualified-repro.log). Native authority still bounded all queries.

The MCP-owned catalog_filter parser checks only those seven representations,
then constructs the existing typed CatalogFilter. Three search adapters reuse
it. Core domain enums/validation, valid strings/null/absent fields, cursor rules,
permissions, outputs, schemas and hello fingerprints are unchanged. This does
not extend v2-only filters or claim full schema equivalence for every nested
input. ADR126 / MCP_OPENING_RESOURCES_V1 were clarified before coding.

Checks actually run on this correction:

- opening_mcp: 10 passed, 0 failed (target/mcp5-filter-shape-fixed.log). The new
  case checks seven malformed filters on both entry points, zero search dispatch
  before valid cases, then nine valid filters per entry (null/absent controls
  and all seven string variants), with unchanged native snapshot.
- Catalog and qualified direct boundary cases: 2 passed, 0 failed
  (target/mcp5-filter-catalog-fixed.log, mcp5-filter-qualified-fixed.log).
- Actual context pipe and catalog/qualified executable stdio cases: 3 passed,
  0 failed (target/mcp5-filter-context-pipe.log, mcp5-filter-catalog-stdio.log,
  mcp5-filter-qualified-stdio.log). The expanded negative manifests reject
  before later valid reads; native context/business bytes stay intact.
- Independent Python jsonschema 4.26.0: 77 schemas OK; each of the three catalog
  manifests now covers 29 documents, including seven new object filters each
  (target/mcp5-filter-shape-schema.log). Optional actual-transcript checking was
  not enabled for this run.
- Scoped Host lib / four affected test targets Clippy, formatting, public-content
  and Runtime surface checks passed; runtime_surface: 4 passed, 0 failed
  (target/mcp5-filter-shape-clippy.log, mcp5-filter-shape-surface.log).

Current integration aggregate:
ecc8332a5025d9b87b776d2bdb2e85400efdd49e5fe8bb3032509c9a2452a455.
No full workspace, NativeMemoryOwner, actual model/Provider, private root or
Runtime product/config/pin was used. No new stage, capture or mandatory queue.

## Search cursor algorithm (2026-11-07)

Claude continued from 5fe7380 after Codex's quota ran out. The cursor was the
remaining enum in the read-tool search inputs: its selection.algorithm is a
string const/enum in all three search schemas, but serde accepted an
externally tagged object and dispatched the search. One new invalid manifest
case per catalog-type corpus ("object cursor/selection/algorithm") is otherwise
schema-valid; replacing the object with its tag string validates.

Before the fix, the focused opening case admitted it on both the context and
initialized entry points (target/mcp-cursor-enum-repro.log). The MCP-owned
json::catalog_cursor parser now checks that one representation and constructs
the existing Option<CatalogCursor>; the three search adapters use it.

Checks actually run on this correction:

- opening_mcp focused case: 1 passed, 0 failed. It now also forms a second
  synthetic memory, takes the actual next cursor from a limit-1 page and
  confirms both entry points return the second page (target/mcp-cursor-enum-fixed.log).
- Catalog and qualified direct boundary cases: 2 passed, 0 failed (same run
  family; the first fixed run).
- Actual context pipe and catalog/qualified executable stdio cases: 3 passed,
  0 failed (target/mcp-cursor-enum-stdio.log).
- Independent Python jsonschema 4.26.0: 77 schemas OK; each catalog-type corpus
  now covers 30 documents (target/mcp-cursor-enum-schema.log).
- Host Clippy for all targets with -D warnings, formatting, Runtime surface
  manifest regeneration and runtime_surface tests (target/mcp-cursor-enum-clippy.log,
  mcp-cursor-enum-surface.log).

Current integration aggregate: 807719f48e232d5b758296562ada3c010deac4635a84856695d88905b1233f1e.
No full workspace, NativeMemoryOwner, model/Provider, private root or Runtime
product/config/pin was used. Domain-wide enum rewriting is still not claimed.
