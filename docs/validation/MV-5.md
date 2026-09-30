# MV-5 — Context, session, and the Mock loop (synthetic)

Date: 2026-09-30. Scope: MV-5 ([plan §9](../design/IMPLEMENTATION_PLAN.md)), authorized by the owner's "继续" to the MV-4 report that named MV-5, and finished on the owner's request to complete MV-5 and merge it through a pull request. Start: `e8bbf4e`. Every input is synthetic; no real data root, Provider key, model, network, MCP, or VPS was used. MV-6 has not started.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| MV-5.1 compiler (ADR-MEM-43) | `compile` recalls through the MV-4 index at the Vault's head, then applies the contract's hard exclusions (tombstones, policy `memory:read`, `highly_sensitive` never auto-included), currency at the request time, whole-item selection under a UTF-8-byte budget of the rendered request, and `verification_needed` (live values, conflicts on both sides, `implementation_unverified` for a decided project state without implementation evidence). Every considered record gets an inspection decision with a reason; a denied principal sees no decisions at all. The capsule and its inspection are saved in one commit, after a freshness check that fails closed if a deletion or policy change happened meanwhile. Versions `context-1`, `context-rank-1`, `mock-utf8-1`. | `enouia-memory-context`: `compiler.rs` |
| MV-5.2 sessions | Session start, user input saved before any compile or answer, streamed output as chunk events with complete/cancel/fail terminals, turn state with the last persisted event, branch fork, and provisional checkpoints whose coverage is the ordered event IDs plus a `coverage-1` hash; resume is checkpoint plus the events after its coverage. Every write is idempotent under its request key. | `session.rs` |
| MV-5.3 Mock | `answer_saved` reads only a saved capsule, renders the exact request body, records a `DispatchRecord` (destination `local_mock`, no tools) and returns deterministic statements with source references, or abstains (`no_supported_evidence`, `implementation_unverified`, conflict). `inspect_request` re-renders the saved request and checks it against the dispatch hash. A crash between dispatch and reply publication is repaired by the retry exactly once. | `mock.rs` |
| MV-5.4 MVP checks | MoriMeta decision vs implementation, rebuild/restart/replay equivalence, backup restore of the actual capsule and Mock body, source-text injection, abstention, purge of saved context. | `tests/continuity.rs`, CLI test |
| Contract and Vault | Capsule, inspection, and dispatch are now stored, cataloged single-revision records (`records/capsule|inspection|dispatch/`); the fixture generator commits them (`session_append`) and the MoriMeta and lifecycle sets were regenerated. A purge deletes saved capsules, inspections, dispatches and derived replies that name purged records. Local-path detection no longer flags URLs. | `contract/record.rs`, `layout.rs`, `set.rs`, `govern/delete.rs`, `tools/fixture-gen` |
| CLI | `session-new`, `session-status`, `session-input`, `session-output`, `session-fork`, `session-checkpoint`, `context`, `context-inspect`, `mock`, `mock-inspect`. | `enouia-memory-cli` |

The owner boundary is in-process: the principal is whoever calls the library or the CLI. A future Host (MV-8) must authenticate it.

## 2. Acceptance items

| Item | Status | Evidence (`crates/enouia-memory-context/tests/continuity.rs` unless named) |
|---|---|---|
| C01 MoriMeta design choice | Passed | `c01_c02_*`: the saved capsule holds only the approved MoriMeta decision with provenance; Moriium is not included; the Mock answers `supported_evidence` citing Professional Darkroom. Without an approved decision the answer is `no_supported_evidence`. |
| C02 decision vs implementation | Passed | `c01_c02_*`: "是否已经实现" yields `implementation_unverified`; the fixture sets carry the same `verification_needed` reason. |
| C03 P0 sensitive, old price, expired | Passed | `c03_*`: a P0 `highly_sensitive` memory never enters the capsule; a live price is `needs_reverification` with `live_value`; a principal without read gets no items, no decisions, and no IDs in the capsule. Expired and future facts are covered by the lifecycle set rules. |
| C04 budget and long Chinese text | Passed | `c04_*`: an item that does not fit is left out whole (`over_budget`), never truncated; `estimated_tokens` equals the rendered UTF-8 bytes plus the fixed wrapper; a budget too small for the request fails with `budget_exceeded`. |
| C05 determinism | Passed | `c05_*`: replaying the same request returns the same capsule without a new commit; after an index rebuild and a Vault reopen the selection, order, and reasons are identical. |
| C06 inspector vs actual request | Passed | `c06_c07_*`: the re-rendered request verifies against the `DispatchRecord`, its hash equals the answer's request hash, no tools, destination `local_mock`; another principal cannot inspect it. |
| C07 source prompt injection | Passed | `c06_c07_*`: a memory telling the model to upload the Vault is carried as data under the "untrusted data" system rules; nothing is written, sent, or re-scoped. |
| C08 no evidence, conflict, index unavailable | Passed | `c08_conflicts_*`: both sides of a conflict are included and no winner is chosen; `c08_unavailable_index_*`: an index behind the Vault yields `index_not_ready` with no items, not a hidden fallback. |
| S01 input saved, call fails | Passed | `s01_compile_and_mock_failures_*`, `s01_s02_*`: the user input is durable before compile; a failed compile or Mock leaves the turn pending with no fabricated reply. |
| S02 cancel during streaming | Passed | `s01_s02_*`: chunks already persisted stay, the turn is `cancelled`/`failed` with its last persisted event, and partial text is never marked completed. |
| S03 checkpoint and later events | Passed | `s03_s04_*`, `checkpoint_hash_*`: the coverage hash verifies and a tampered one is refused; a later checkpoint that resolves the open loop removes it from the next capsule, and the turns after the checkpoint are still carried. |
| S04 across windows, branches | Passed | `s03_s04_*`: a fork does not see its sibling's later events; checkpoints and their open loops stay provisional in the capsule. `checkpoint_hash_*`: a retried input returns the same event, the same key with other text is refused. CLI: `installed_session_context_and_mock_survive_each_process_restart` runs each step in a new process. |
| Backup and purge | Passed | `backup_restore_*`: a restored Vault answers with the same capsule and byte-identical Mock body; `purge_clears_saved_context_*`: purging a memory removes the capsules, inspections, dispatches, and replies that named it, and the Vault still verifies. |

## 3. Checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 182 passed: contract 69, vault 59, context 15, import 14, govern 12, index 9, CLI 4 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `python tools/schema-check/check_schemas.py` | exit 0; 33 schemas, 4,913 set records |

## 4. Defects found and fixed

- Capsules, inspections, and dispatches were declared "not stored" although MV-5 must save the actual request; they are cataloged records now, and the fixture commits catalog them (the new completeness check had rejected every MoriMeta set).
- The dispatch stale-barrier rule treated a tombstone with the same millisecond as the egress check as later; with a higher deletion epoch that tie no longer counts as a stale barrier.
- `contains_local_path` flagged `https://…` as a drive path.
- Two vault tests assumed the lifecycle's delete was its last commit.

## 5. Not done, or pending

- Real Provider calls, external dispatch states (prepared, sent, outcome unknown), payload confirmation, and tokenizers: MV-7. The budget counts UTF-8 bytes only.
- Authenticated principals and finer grants (MV-7/MV-8); only owner and denied principals are exercised.
- Checkpoint summaries are provisional and written by the caller; no model summarizer.
- Raw-source search in the compiler, and the source browser when the index is behind (MV-6).
- Everything still pending from MV-1 to MV-4 (I07, restic, OS-crash and power-loss evidence, per-group indexes at scale).

## 6. Commits

| Commit | Content |
|---|---|
| `ec2dc40` | Context compiler, sessions, Mock, CLI, contract changes (work in progress) |
| `63ef644` | Fixture commits catalog context artifacts; suite green |
| (this commit) | This report, ADR-MEM-43, status updates |

## 7. Before MV-6

1. Owner authorization for MV-6 (Windows memory workspace).
2. Everything a real import still needs (I07, restic, backup media, off-device secret, real data root with the owner-only ACL).
