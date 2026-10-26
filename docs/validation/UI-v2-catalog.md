# UI v2 catalog validation: G01 → G06 → G23

2026-10-26. ADR-MEM-94. Stacked on accepted record contract
`24a03e02bdf18094d60bc4d3268d380bd1b669a1`, from verified main
`c9bf1be7819dba19915adf6031b1900c032b73d1`.

## Scope and mapping

| Handoff | Delivered native behavior |
| --- | --- |
| 01 G01 | New-empty segmented root, 20,000 actual synthetic machine records, stable paginated IDs, exact totals, 100-row/byte limits |
| 01 G06 | Incrementally materialized literal title/content index; Chinese/English aliases; form/state/knowledge/adoption/validity/attention/entity/project/theme/time filters; nine authorized facet axes |
| 01 G23 | Pure exact statistics over the same current authorized set and filters, explicit scope and bounded group continuation |
| 01 G18 | Algorithm/version, snapshot, withdrawal watermark, authorized-set/query/profile binding, fresh permission/deletion checks, no write side effects |
| 02 §3–§7 | Existing accepted record fields remain unchanged; this slice supplies their scalable persistence/read use |

Physical layout is **automation-segmented-host-v1**, schema 2. This additional
layout is necessary because HostV3's full-snapshot history is bounded at 128
entries/64 commits/32 MiB. No existing root is converted. Native indexes rebuild
from a canonical linked delta chain and are not a second authority database.
Original requests, raw generation responses and witnessed receipts survive
reopen. Publication faults fence the instance; recovery is explicit and forward.

## Checks actually run

Pinned Rust 1.98.1 on Windows GNU, `CARGO_NET_OFFLINE=true`, locked dependencies.

- `cargo fmt --all -- --check`.
- `cargo test --workspace --locked`: 683 passed, 0 failed; the separate scale case is ignored here and was explicitly run above.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`.
- Independent Python 3.12 / python-jsonschema 4.26.0:
  `tools/schema-check/check_schemas.py`: 52 schemas; UI record corpus 50 documents;
  catalog corpus 33 documents (9 positive, 24 negative); all passed.
- `python tools/integration/runtime_surface.py`: manifest matches and aggregate
  `f02fe5f5de82ea141f509a3693b8f058e30f69268a6708ffda8bf1b1203b5d09` is logged.
- Explicit `cargo test --release -p enouia-memory-host --test catalog --locked
  twenty_thousand_native_records -- --ignored --nocapture`: passed. 20,000
  witnessed machine records; 244 initial pages; write 178.1863963 seconds;
  whole test 266.48 seconds under concurrent local validation. Deletion/revocation
  invalidate old cursors; reopening/traversing returns exactly 19,936 records.
  These figures are observations, not a performance guarantee.

The scale corpus has real exact source policies and machine batches, not
injected read DTOs. It exceeds the old source/commit bounds, verifies no ID
omissions/duplicates, and compares Chinese/form/adoption-filtered traversal
with statistics. Its final run preceded only an added attributed-subject check
on entity evidence; the scale corpus has no entities. Final focused/full tests
exercise that check and the original scale behavior remains applicable.

Focused regressions cover locked readers, current sensitivity ceilings,
profile/filter/cursor changes, expiry/deletion/revocation, bilingual aliases,
entity evidence under the memory's own policy/subject, stale indirect lineage,
event versus learned/captured axes and unknown time, progressing facet pages,
read-only file contents, >128 native records/reopen/receipts, exact publication
fault recovery/corrupt-segment refusal, and old-reader rejection. A same-capture
source replacement cannot lend its old revision to newly appended metadata.

During validation, the read-purity assertion initially assumed `indexes/` was
absent/empty. Managed roots already create directories and the Host lease there.
The corrected assertion compares all actual file contents before/after reads,
while preserving the exact intentionally unreadable held lease filename.

## Differences and remaining work

- Counts are exact **current authorized machine revisions**, not legacy owner
  history, all captured material, or the owner's entire life.
- Search is literal plus explicit aliases; no embedding/model call or linguistic
  inference. Entity/alias dependencies conservatively withhold a row if hidden.
- Theme filtering currently means `recorded_derivation_only`; G05 must add
  scalable rebuildable families. No inferred theme membership is claimed here.
- Source/history detail, Context delivery, time buckets, graph, activity/change
  feed, conflict groups, owner actions and Runtime UI mounting are not supplied
  by these three ports. Owner ledger execution remains denied until G08.
- Immutable history retains bytes; whole-filesystem rollback, physical purge,
  real initialization, real Provider quality, Runtime UI acceptance and deployment
  are not proven or performed.

Only new isolated OS-temporary synthetic roots and owned test processes were
used. No Runtime checkout/pin, private Vault/export/credentials, main checkout
edits, MCP worktree files or real provider were touched.

Next: G03 time buckets → G05 family projection → G04 typed graph, with explicit
query timezone and algorithm/coverage metadata. MCP uses ADR95/96 on its own
concurrent branch; this session will use the next available ADR number.
