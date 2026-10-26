# MCP-2c checkpoint 2: segmented Agent five-tool reads

Started 2026-10-26, final validation 2026-10-27. Base: native gate `ec82a28d975a18d759a6bf83beec439ad1136868`
on the exact published segmented dependency `b2c5c1a`. No main merge.

## Delivered surface

`SegmentedAgentHost` shares the existing leased Vault through Arc; it opens no
writer. Native Owner registration and retained OS peer enrollment are separate
prerequisites. Its own lock/unlock generation, revocation, injected/monotonic
expiry and current policy/source/lineage gate precede all result construction.
Native embedding must connect its Agent lifecycle to this object; an existing
System reader's lock alone does not grant or manage Agent authority.

Five explicitly versioned tools return complete frozen record/source fields,
producer/model versions, authorized entity evidence, literal/alias catalog
filtering and in-scope counts. History, source permission, UTF-8 excerpt limits,
hint-endpoint filtering and bounded provenance neighbors remain explicit.
Search cursors bind current snapshot, withdrawal watermark, delegated scope,
query/profile and stable evaluation time, and always progress when trimmed.

The explicit Core-6 catalog API ranks all authorized candidates at once using
the existing deterministic algorithm. The old API remains bounded to 128.
Catalog trace returns all selections, exact considered/rendered-byte counts,
at most 128 excluded identities and exact exclusion reason counts with honest
coverage. Native Context receives the identical cached response and rechecks
its snapshot and authority. No read reinforces attention or updates business
files. Native per-tool counters remain ephemeral, distinct from Activity/Usage.

The stdio executable accepts `--protocol automation-catalog-v1` with only the
existing pipe/host-PID/Vault-ID attachment flags. Identity is never taken from
those flags or clientInfo. Unknown IO outcomes retain the original packet/key,
close the client and never reconnect or resend automatically.

## Version / capability / writer matrix

| Root | Agent reader / local protocol | Writer / rejection |
| --- | --- | --- |
| schema 1 / host-current-v2 | Existing six tools / memory-agent-pipe-1 | Existing restricted proposal/checkpoint semantics only |
| schema 2 / automation-host-v2 | memory-agent-read-1 / pipe-2 | System automatic pipeline; old six tools refuse |
| schema 2 / automation-host-v3 | memory-agent-record-read-1 / pipe-2 | System frozen-field pipeline; v1/HostV2 readers refuse |
| schema 2 / automation-segmented-host-v1 | memory-agent-catalog-read-1 / pipe-3 | Existing single segmented System writer; all three older readers refuse before business decode/query/count |

System memory-native-read-2 remains a separate capability. Pin upgrades do not
adopt new formats or confer permissions. Catalog explicitly allows 4096 unique
requests within the same maximum five-minute enrollment; older readers keep 256.

## Actual synthetic evidence

- New temporary roots, 180 legally formed/witnessed records across three
  policies. Paginated traversal with 16-KiB pages reaches every authorized row.
- Same-row parity against native catalog for literal text, form, adoption,
  knowledge, entity absence and each time axis with/without unknown times.
  This is a test comparison; Agent production code never calls System reads.
- 100 repeated searches plus native detail/recall checks compare every readable
  business file byte-for-byte. Only OS-held `indexes/host.lock` and `vault/LOCK`
  are excluded because they cannot be opened while leased.
- Independent owned Agent process over a real protected pipe invokes all five
  tools and a budget-excluded recall. Context response equals the actual MCP
  result. Bundles contain reachable local refs only.
- Legacy six-tool, HostV2 and HostV3 clients refuse the segmented handshake.
  Forged principal fields refuse; no source permission returns a known denial
  while preserving a valid connection. Source deletion removes rows/counts and
  selected detail. More than 256 requests work only on the new capability.
- Lock, explicit revocation and expiry fence live connections, preserve the
  original packet on failure and never reconnect. Owned helper processes exit
  and are reaped. The actual stdio executable attaches to the existing Host and
  releases on EOF without opening a Vault writer.
- Core-6 ranks a best candidate beyond position 128 and preserves exact old/new
  selection parity for bounded inputs; no per-page or per-chunk ranking.

Final checks actually completed with the pinned offline/locked toolchain:

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace --locked -- --test-threads=1`: **728 passed**, zero
  failed, complete doc-test tail and recorded exit code 0.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Python 3.12 / python-jsonschema 4.26.0: 55 schemas, 14 new catalog Agent request
  cases, six actual real-pipe exchanges against their bundled input/output
  schemas, and one deliberately invalid actual-trace mutation; no disagreements.
- `python tools/integration/runtime_surface.py`: logged aggregate
  `551d4fe38bcb47b032806e27fda2c28e521c3c436800ea68676246154364a71b`.
- Staged whitespace/public-content checks: passed; no AGENTS/manual snapshots.

An initial full link failed due to disk exhaustion. Only this worktree's resolved
`target/debug/incremental` cache was removed, preserving source and evidence.
The final run used `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2` to bound cache
and concurrent linker space. An interrupted/incomplete run and a run exposing
the two fixture defects below are not counted as successful full validation.

The full check exposed two pre-existing test-fixture defects. The job status
API intentionally returns Busy under mutex contention, but one assertion
unwrapped it; the fixture now condition-polls only that read-only observation.
Owned crash children previously used PID/counter-only temporary names; captured
stderr proved Windows PID reuse hit an existing crash directory (error 183),
yielding exit 101 instead of the expected injected abort 3. A random suffix
creates a genuinely new root without deleting old directories. Diagnostics
remain attached to failed child assertions. Fifteen focused crash runs and the
invalidation test passed after these test-only fixes. No product retry, writer
lock, receipt or unknown-outcome handling was weakened.

## Differences from Runtime requirements 03 and next slice

This implements the stored-field segmented read baseline, not every new UI
projection. Related exposes recorded provenance only; fact/suggested graph
surfaces and G03 buckets/G05 inferred families/G04 graph/process-detail adapters
remain explicit follow-up work from their separately published contracts.
Excluded-identity detail beyond 128 is not paginated in this capability; only
its exact in-scope reason counts remain available. That limitation is explicit
in the actual trace coverage and is not complete legacy-trace acceptance.
Runtime does not yet bind or render the actual Context cache; backend equality
is proven, frontend acceptance is not. Agent transport tests use 180 records;
the upstream 20,000-row System benchmark is not claimed as Agent acceptance.

Runtime-only conversation capture, live formation, memory_note and preflight,
reports/forget requests, session_start and resources are not delivered here.
Agent statements remain Agent provenance; no Owner action goes through MCP.
Missing timezone will default to Asia/Shanghai with a recorded default source
at capture, without inventing unknown event dates. No private root initialization
is needed to continue that backend work. No real Provider/embedding, private
Vault/export/credentials, Runtime edit, client configuration, remote listener,
deployment or production activation was used.

Owned files: the new segmented Agent Host and mcp/catalog modules, their schema,
request fixtures/tests/contract/report; minimal registration/shared schema/stdio
edits, explicit Core-6 catalog API/test, the two diagnosed existing test-fixture
fixes, ADR/design integration references
and regenerated Runtime manifest/log. No AGENTS, manual status, existing System
read/projection/process methods or parallel uncommitted files were changed.
