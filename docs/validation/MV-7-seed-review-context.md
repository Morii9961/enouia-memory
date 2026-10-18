# MV-7 seed review context and episode recall

2026-10-18. Follow-up to [private seed staging](MV-7-private-seed-staging.md),
within the same owner-authorized migration and Runtime integration slice.

## Changes

Candidate results now expose recorded context dependencies separately from
citations. Citation rows and explicit source excerpts report their saved
evidence class. These additive result fields retain 47 commands and unchanged
request and record schemas. The reference harness reads a selected context
source only on request and checks its exact source ID/revision. Dependencies
do not assert corroboration, availability, approval or complete reading history.
The matching [compatibility log](../integration/RUNTIME.md) and generated
surface manifest record Runtime's adoption requirements in this commit.

The portable preparer previously treated an episode's occurrence interval as
the claim's validity interval. Past events would consequently expire from
ordinary current recall. Occurrence dates now stay in `occurred_start/end`;
claim validity stays unbounded unless separately established. A synthetic CLI
journey stages and approves a 2020 episode and finds it through ordinary search.
The preparer test separately checks the inclusive Asia/Shanghai date conversion.

## Private evidence

Twenty-six affected pending episode candidates were revised through native
proposer transactions, retaining IDs, provenance and old revisions. All 185
candidate acceptance plans still built without confirmation; Canonical and
owner-review counts remained zero. No real candidate was approved.

An inactive restored copy was read with the new Core: 185 candidates, 32 P0
core candidates and 215 recorded context references were observed. A selected
context source retained `unknown` speaker role/evidence class rather than being
upgraded to user evidence. This is Core-read evidence, not rendered acceptance.

The full private bootstrap was additionally copied out of its disposable Git
worktree into the private data area. All 1,897 files (1,786,238,751 bytes) matched
SHA-256; source file set, sizes and modification times were unchanged during
the copy. This preservation copy is not independent disaster recovery.

## Verification

Rust 1.98.1, locked dependencies and offline Cargo were used.

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked`, `RUST_TEST_THREADS=2` | 398 passed, zero failed, including both CLI journeys |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| Python 3.12 preparer tests | Four passed |
| Pinned Python 3.12 jsonschema cross-check | Passed, 36 schemas |
| `python tools/integration/runtime_surface.py` | Passed; aggregate `f1d99344c596ba71d4f1c6d0abcc288a8284def4dcf5aec70b4e1ca3e997fff9` logged |
| Reference shell TypeScript/Vite build | Passed |

Two test threads bound independent harness scheduling; the production code,
ordered race barriers and deadlines were not changed. The earlier default-load
timeout remains historical evidence, not a fixed concurrency defect.
Runtime's separate consumer commit records its pin, frontend checks and native
acceptance limits. No Provider call, cloud activation or new MV stage follows.
