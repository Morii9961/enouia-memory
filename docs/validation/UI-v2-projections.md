# UI v2 projection validation (G03 → G05 → G04)

2026-10-26, ADR-MEM-100, stacked on catalog commit
`b2c5c1a7e806fa5aeda9b74e35241dfbefd26acb`. Schema 2,
`automation-segmented-host-v1`. No stored record field/root conversion.

## Handoff mapping and limits

| Requirement | Native synthetic result |
| --- | --- |
| 01 G03 / 02 rich time | Explicit IANA timezone/day/month/window; possible interval overlap, unknown time, separate capture-material and staged/adopted counters |
| 01 G05 | Stable recorded theme IDs, >128 members, qualified recorded versus shared-entity suggestion scores, catalog/filter/statistics consistency |
| 01 G04 | 5,000 compact nodes, typed provenance versus suggestion edges, exact selected-window candidate/retained counters and progressing byte/row-bounded chunks |
| 01 G18 | Fresh authorization before every count/label/group/edge, versioned selection/parameters, deletion invalidation and no persisted authority cache |

Catalog algorithm is now literal-family-catalog-2/version 2. Record fields stay
at ADR93; physical layout stays at ADR94. Old v1 catalog fixtures remain valid
with their earlier meanings, but cannot continue under the new native selection.

Ordinary pages and selected family reads use shared/lazy seed views, not an
allocated full membership cross product. Whole-theme aggregation and graph
work bounds are explicit; exceeding them refuses without partial exact counts.
Only non-subject shared entity IDs make suggested family links. This is a
published deterministic heuristic, not semantic clustering/model quality.

Owner names/pins/dissolution and their inheritance are not claimed tested here:
owner ledger execution is still denied, and G08 must supply actual native
confirmation plus overlay checks. Historical revision/owner correction edges
belong to G15/G08, not invented self-edges between identical current node IDs.
Semantic neighbors remain explicitly unavailable. No Runtime mounting or visual
acceptance is implied.

## Checks and evidence

Final pinned Rust 1.98.1 checks passed:

- `cargo fmt --all -- --check`.
- `cargo test --workspace --locked`: 687 passed, 0 failed; both large native
  cases are ignored here and were actually run separately as recorded below.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Independent schema check: 53 schemas, catalog corpus 37 documents and
  projection corpus 31 documents (3 positive, 28 negative), all passed.
- Integration generator/check: aggregate
  `d64a21005cf3f60b71dce246f4644be74894860e8a360bd4e005f06b918ad2d6`
  matches and is logged in RUNTIME.md.
Independent Python 3.12/jsonschema 4.26.0 checks the separate projection schema
and synthetic positive/negative corpus in addition to native validation.

Native tests cover UTC 16:00 around the Shanghai local-day boundary, distinct
unknown event time, interval overlap versus capture/learning time, Berlin 23/25
hour DST days, Pacific/Apia's skipped date, 366 leap-year day buckets, month
buckets, unknown-zone refusal and lock. They also form 201 native records with
public/private ceilings, then a theme with 173 authorized members. Facets/
statistics match that family; hidden members never enter graph counts/endpoints.
Deleting a member changes the count; reopening preserves the family ID;
deleting its label-evidence source removes the family and dependent graph.
A separate conversation test requires a current authorized conversation and
both endpoints, and removes relations after dependency/sensitivity changes.

Explicit release test (ignored in the default suite, actually run separately):
`cargo test --release -p enouia-memory-host --test projections --locked
five_thousand_graph_nodes -- --ignored --nocapture` passed: 5,001 real native
synthetic records, a 5,000-node stable-ID window, 50 pages, no omissions or
duplicates, explicit incomplete window coverage, and deletion invalidating the
old cursor. The test ran in 29.53 seconds. It uses real witnessed delta commits,
not injected graph DTOs. Typed-edge clipping is tested on the smaller dense
entity/theme fixture with exact clique candidates and retained counts.

Timezone data is compiled from exact jiff-tzdb 0.1.8, IANA release 2026c; its
bytes feed exact Jiff 0.2.37's TZif parser. No OS timezone/zoneinfo is read.
The APIs were checked in the locally locked sources and official
[Jiff timezone](https://docs.rs/jiff/0.2.37/jiff/tz/struct.TimeZone.html) and
[civil date](https://docs.rs/jiff/0.2.37/jiff/civil/struct.Date.html) documentation.
Only already locked package versions gained Host dependency edges; no provider,
model, embedding, private root/history, Runtime/MCP checkout or deployment.


The first final workspace attempt hit E-drive exhaustion during compilation
(`No space left on device`), so it is not a passing run. Only this worktree's
rebuildable Cargo debug cache was removed after verifying its exact resolved
path and Cargo cache marker; other worktrees/data were untouched. Final tests
use process-local `CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, and two build jobs. Toolchain, offline/locked mode,
all workspace members, test selection, ACL checks and product logic are unchanged.

The updated lazy catalog algorithm's explicit 20,000-record release regression
also passed: 244 initial pages; native write 118.8871606 seconds; complete run
203.21 seconds; exactly 19,936 records after deletion/revocation and reopen.


Before publication the concurrent MCP branch added ADR97. This slice was
renumbered to ADR100 without touching its files. Only ADR/document references,
the schema title and resulting integration digest changed after the full run;
the focused integration check and independent schema cross-check were rerun.
