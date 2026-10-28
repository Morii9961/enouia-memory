# UI v2 process validation

2026-10-27, ADR-MEM-101, based on projection commit
`2ed88f4177cf2ae0fecd64ae27050d88df3a27c2`. Schema 2 and
`automation-segmented-host-v1`; accepted ADR93 fields unchanged.

| Handoff requirement | Native result / qualification |
| --- | --- |
| 01 G07 | Witnessed capture/admission stream, actual publication actor/time and receipts; no fabricated time-derived jobs |
| 01 G13 | Indexed ancestor anchor, scoped appended IDs, explicit reload on barriers/revision/overflow; no global sequence count |
| 01 G15 | Current counter-source overlap group, adjacent authorized structural diff and typed selected revision neighbors |
| 01 G16 | Verified sensitivity envelope on v3 rows/graph, detail/history/source/activity/session outputs |
| 01 G19 / 02 §6 | Recorded interpretation/prompt/rule/model/mode/batch/note, explicitly machine annotation; null if absent |
| 01 G14 / 02 §3 | Current conversation metadata, captured fragment union, distinct turn/source counts and exact Unicode-scalar quote jump |
| 01 G18 | Fresh authorization before titles/counts/history/edges, versioned snapshot bindings and deletion invalidation |

Native synthetic tests exercise public/private visibility, a private fragment
containing public sources, hidden-parent exclusion, lock, current source deletion,
reopening, real correction receipts and private sensitivity under a wider policy.
150 actual captured turns span three immutable conversation revisions; 150 are
pageable under Private and only 86 under Public because the middle fragment is
Private. No hidden fragment metadata is declassified by public source bodies.
Selected Unicode excerpts match exact scalar ranges and anchor hashes.
Activity title/event counts exclude hidden publications; scoped no-change and
snapshot invalidation are distinct. A machine note containing “Owner confirmed”
remains an annotation, with a System actor and no owner-action record.

The contract corpus has nine native-emitted positive documents and 69 negative
mutations, including forged owner actor, missing qualifications, false counts,
stale header/anchor claims, nonadjacent diff, unqualified suggestions and invalid
source/turn bindings. Native construction, Rust subset schema validation and
independent Python JSON Schema validation are separate checks.

## Verification

Final pinned Rust 1.98.1 checks passed with offline/locked resolution:

- `cargo fmt --all -- --check`.
- `cargo test --workspace --locked`: 691 passed, 0 failed, 3 intentionally
  ignored (two scale cases and the explicit fixture emitter).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Independent Python 3.12/jsonschema 4.26.0: 54 schemas, catalog 41 documents,
  projection 31 documents, process 78 documents; all passed.
- Runtime surface generator/check: aggregate
  `191617ddd3d82065a5012e70b644a7ac69d6768b02216739da6ce410f1912599`,
  logged in RUNTIME.md in this commit.

All Cargo checks used process-local incremental/debug-information disabling
and two jobs, preserving pinned toolchain, workspace selection, assertions and
native ACL checks. Earlier clippy style findings were fixed before the final
passing run. One release build overlapped source updates and failed with mixed
interface versions; it is not passing evidence. The final source was frozen
before final full checks and a fresh release run.

The explicit ignored release scale case was also actually run:
`cargo test --release -p enouia-memory-host --test catalog --locked
 twenty_thousand_native_records_paginate_exactly_and_survive_reopen -- --ignored --nocapture`.
It passed with 20,000 actual native synthetic records, 271 initial pages,
complete filtered traversals, old-cursor rejection after deletion/revocation,
and exactly 19,936 records after reopening. Native writing took 203.4918819 seconds; the complete test took 306.64 seconds
under concurrent workspace validation. This checks storage/index/v3 regression,
not initialization or real data/provider quality.

## Limits and differences from the request

Activity contains actual witnessed events for currently eligible dependencies;
no real fading/archive job exists to supply those suggested events. Owner events
wait for G08. Current deletion removes historical titles instead of returning a
title-bearing tombstone. Change anchors use exact snapshots with scoped IDs and
reload, without an observable global sequence or hidden change count.
Conflict membership is declared counter-source overlap, not semantic verification.
History can disappear when original dependencies become stale or unauthorized.
Unavailable/deleted/ungranted sources share one empty response, so Runtime should
show source unavailable without asserting the private reason.
These are native backend results, not Runtime mounting, visual acceptance,
initialization, real provider evidence, owner acceptance or deployment.
