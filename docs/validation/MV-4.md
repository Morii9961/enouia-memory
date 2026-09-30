# MV-4 — Index and retrieval (synthetic)

Date: 2026-09-30. Scope: MV-4 ([plan §8](../design/IMPLEMENTATION_PLAN.md)), authorized by the owner's "继续" to the MV-3 report that named MV-4. Start: `d3f2948`. Every input is synthetic; no real data root, model, embedding, network, MCP, or VPS was used. MV-5 has not started.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| MV-4.1 index projection (ADR-MEM-42) | `indexes/memory.sqlite`: one row per memory revision with the commit range in which it was the latest (so `known_at` needs no second index), projects, tombstones, applied commits. The watermark advances with each transaction (up to 256 commits, cancellation between), an index of another Vault, format, folding version, or chain, or one failing `quick_check`, is refused and rebuilt. Purges remove rows and FTS entries, merge FTS segments, and vacuum with `secure_delete`. | `enouia-memory-index`: `store.rs` |
| MV-4.2 Chinese and mixed search | Literal query, folded (`fold-1`: width and case only); terms of 3+ characters through FTS5 `trigram` as quoted phrases, 1–2 characters through a bounded substring scan (projects first), ASCII words also through `unicode61`; project name and alias recall; overlapping project names reported as ambiguous, never merged. | `fold.rs`, `search.rs` |
| MV-4.3 permissions and history | Filters before anything is returned: types, projects, current tombstones for every snapshot, broken provenance, `memory:read` against current policies (unreadable projects are not even named), `as_of` currency. Pages re-read from the Vault; cursors bound to snapshot, epochs, principal, filters, and ranking; `index_not_ready` when behind. | `search.rs` |
| MV-4.4 frozen ranking | `rank-1`: named-project hit, currency, whole-word hit, text position, priority, business date, memory ID, revision; no raw scores added. Golden set `tests/fixtures/memory/search-golden.json` (17 queries); timing baseline example. | `search.rs`, `examples/measure_search.rs` |
| Supporting changes | Governance: `new_project` in approved fields creates the project with the accepting review (needed for aliases). Vault: `read_parsed` from the verified cache, purge coverage cached per commit. CLI: `index`, `index-rebuild`, `search` (brings the index to the head first). | `govern/review.rs`, `vault/store.rs`, `vault/purge.rs`, CLI |

No embedding, cloud search, or automatic raw-text injection exists. Search covers canonical memories only; searching raw sources ("原文提到") is a separate, later view.

## 2. Acceptance items

| Item | Status | Evidence (`crates/enouia-memory-index/tests/search.rs` unless named) |
|---|---|---|
| R01 delete, corrupt, rebuild | Passed | `r01_*`: a deleted index is empty and `index_not_ready` until it catches up; a corrupted file is refused (`index_not_ready`), the Vault verifies clean, and a rebuild answers the same; `golden_*`: the whole golden set is identical before and after a rebuild. Review state lives in the Vault only. |
| R02 short Chinese words and mixed text | Passed | Golden set: 记忆 (two characters, scan), 茶 (one), 琉璃光院 (trigram), 函馆 and 函館 kept distinct (no variant conversion), MoriMeta, full-width ＭｏｒｉＭｅｔａ, English words, mixed "agent 预算". |
| R03 aliases, same-name entities, ambiguity | Passed | "MoriMeta" and its alias "森元" recall the project's memories first; "mori" reports both MoriMeta and Moriium as ambiguous and chooses neither; nothing is merged, and colliding names are refused at review (`project.alias_collision`). |
| R04 ACL and snippet/count/cursor | Passed (allow/deny principals) | `r04_*`: a principal without read permission gets no items, no project names, no cursor, and no `partial` flag for any query; a forgotten memory disappears from every query. Finer grants (per sensitivity or project) cannot be created yet (owner grant approvals come with MV-7), so they are not tested. |
| R05 lagging index | Passed | `r05_*`: after a new commit the search refuses with `index_not_ready`; a cancelled update stays behind and still refuses; after the update the new memory is found. Results are never mixed across versions. |
| R06 input and pagination | Passed | Golden set: ASCII quotes, `*`, `') OR 1=1 --`, `NEAR(…)` are literal text; `r06_*`: pages continue their first snapshot after later commits and together equal the full result; a cursor used with another query or principal, or after a deletion, is refused; a tampered cursor is invalid; empty queries and limits over 100 are refused. |
| History and purge | Passed | `known_at_*`: a search at an earlier commit does not see a later approval; `as_of_*`: a fact not yet in effect is hidden now and shown at its date; after a purge the text is gone from the index file (positive control first), also after a rebuild. |

## 3. Measurements

`measure_search` (synthetic, release build, Windows 11, NTFS, SSD; each memory is an owner statement, a candidate, and a reviewed batch of 50). Two runs; the second ran while the test suite was compiling and running, so its times are high; both are given.

| Step | Run 1 (per-commit transactions) | Run 2 (256 commits per transaction, loaded machine) |
|---|---|---|
| Writing 1,000 memories (2,021 commits) | 175.9 s (about 87 ms per memory) | 221.9 s |
| Index rebuild over 2,021 commits | 25.10 s | 1.18 s |
| Incremental update of one commit | 7 ms | 17 ms |
| Query latency, median of 7 warm runs, 6 queries (short scan, trigram, project name, one character, two English words, mixed) | 22.5 to 24.9 ms | 60.0 to 73.6 ms |
| Index file | 2.66 MB | 2.68 MB |

Query time is dominated by parsing every visible memory row for currency (supersession needs the latest set), not by FTS; it grows linearly with the number of memories.

## 4. Checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 161 passed: contract 68, vault 55, import 14, govern 12, index 9, CLI 3 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `python tools/schema-check/check_schemas.py` | exit 0; unchanged contracts |

## 5. Defects found and fixed

- Internal claim keys were indexed as full text, so "mori" or "memory" matched words the owner never wrote; only readable fields are indexed now.
- The SQLite build failed because the Rust toolchain's older MinGW runtime DLLs came first in `PATH`; the requirement is documented in the README and ADR-MEM-42.
- Governance read every candidate and memory from disk on each proposal; reads now use the Vault's verified-record cache.
- A rebuild paid one synchronous flush per commit (25.1 s for 2,021 commits); commits are now applied in transactions of up to 256 (1.2 s).

## 6. Not done, or pending

- Raw-source search, searching candidates, and the source browser offered when the index is not ready (MV-5/MV-6 surfaces).
- Finer-grained permission tests (R04) once owner grants exist (MV-7).
- NFKC folding (no tables available offline) and any variant conversion, deliberately.
- Write cost: every commit still loads all small-kind records (ADR-MEM-39 trigger: about 20,000 revisions); at 1,000 memories writing one reviewed memory costs 87 to 111 ms (three commits). Query cost grows with the number of memories (23 to 74 ms at 1,000, depending on load). Both need per-group indexes before a library of tens of thousands of memories.
- Everything still pending from MV-1 to MV-3.

## 7. Commits

| Commit | Content |
|---|---|
| `73b6af3` | Index crate, search, golden set, CLI commands, governance project creation, read caching |
| (this commit) | This report, faster rebuild, status updates |

## 8. Before MV-5

1. Owner authorization for MV-5 (context, session, and the Mock loop).
2. Everything a real import still needs (I07, restic, backup media, off-device secret, real data root with the owner-only ACL).
