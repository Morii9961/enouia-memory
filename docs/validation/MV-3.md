# MV-3 — Canonical memories, candidates, and governance (synthetic)

Date: 2026-09-29. Scope: MV-3 ([plan §7](../design/IMPLEMENTATION_PLAN.md)), plus the segmented catalog that the MV-2 report named as the first prerequisite. The owner authorized it by replying "继续工作吧，不要停下来" to the MV-2 report that named MV-3 and recommended the segmented catalog first. Start: `d9f9941`. Every input is synthetic; no real data root, export, model, network, MCP, or VPS was used. MV-4 has not started.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| MV-3.0 segmented catalog (ADR-MEM-39) | A commit is a stored commit that references immutable, content-addressed catalog segments; each record kind's IDs and the object hashes are split by hex prefix into the canonical partition for a capacity of 512, and a commit rewrites only the leaves it touches. Entries carry every earlier revision hash and group IDs. `read_manifest` still returns the complete v1 view. Descriptor and export format 2. | `contract/catalog.rs`, `vault/store.rs`, `contracts/store/{catalog-segment,stored-commit}-v1` |
| MV-3.0 scoped validation (ADR-MEM-39) | Each commit is validated on a scoped set (every small-kind record, only the bulk documents the delta's rules read, whole groups when a group rule applies), without and with the delta; only added violations count. New rule `import.input_changed`. | `contract/delta.rs`, `set.rs` |
| MV-3.1 candidates (ADR-MEM-40) | `propose`, `edit_candidate`, `withdraw`, `pending_candidates`. Evidence, its class, anchor, and locator come from the cited source revision; sensitivity never below the strictest source; stale targets refused; pending duplicates found by fingerprint; contradicting active memories listed as conflicts. | `enouia-memory-govern`: `propose.rs`, `evidence.rs` |
| MV-3.2 review | `plan` / `confirm`: the exact records a commit writes, hashed without volatile timestamps; owner of the Vault on the same trusted surface within 10 minutes; rebuilt on the current head and committed all-or-nothing with every candidate and target revision expected; single-use nonces; replays. Accept, edit-accept, reject, merge (into a memory or a candidate). | `review.rs`, `view.rs` |
| MV-3.3 change and conflict | Supersede (explicit edge, confirmed effective time, old revision superseded in the same commit), archive, revise, conflict groups (no winner by time or confidence), identity changes with the Markdown in the plan, and a store guard: only the Vault owner may commit memory, review, identity, project, tombstone, purge receipt, approval, and policy records. | `review.rs`, `vault/store.rs` |
| MV-3.4 data lifecycle (ADR-MEM-41) | Owner-only forget (logical delete) and purge through the same plan; purge preview; intentionally purged content in the Vault (reads, verify, export, sweep, validation); `purge_files`; honest purge receipts; deletion ledger and `reconcile_deletions` for restored backups. | `delete.rs`, `vault/purge.rs` |
| MV-3.5 management entry | CLI `candidates`, `memories`, `remember`, `review`, `forget`, `purge-preview`, `purge`, `deletion-ledger`, `reconcile`; review commands print the exact plan and need its typed code. | `enouia-memory-cli` |

Nothing accepts a model proposal by itself, uses confidence as approval, or imports into canonical memory in bulk.

## 2. Acceptance items

| Item | Status | Evidence (`crates/enouia-memory-govern/tests/…` unless named) |
|---|---|---|
| M01 unapproved, rejected, withdrawn candidates | Passed | `review.rs::m01_*`: pending, rejected, and withdrawn candidates never appear in the canonical view; one accepted candidate does. Model requests do not exist yet (MV-5/7); they will read this view. |
| M02 forged approval, imported "remember" | Passed | `review.rs::m02_*`: an agent cannot plan or confirm, an owner-typed actor with another principal is refused, a confirmation by an agent is refused, an agent cannot propose a delete; an imported "记住……" message is a source only. `changes.rs::m08_*`: an agent's direct identity commit is refused by the store (`store.owner_only_kind`). |
| M03 edits and stale batches | Passed | `review.rs::m03_*`: edit-accept stores the owner's text, the review hashes it, the candidate keeps the proposal; a batch whose candidate moved after the plan writes nothing and leaves both pending; a repeated confirmation replays; an expired plan or another surface is refused. |
| M04 evidence roles | Passed | `review.rs::m04_*`: a citation of an assistant message stays `model_claim` through proposal and acceptance; every accepted memory's evidence resolves; lower sensitivity and missing sources are refused. |
| M05 supersession | Passed | `changes.rs::m05_*`: new and old revisions kept, the old one superseded; stale, missing, and cross-scope targets refused (the last at commit, `supersession.cross_scope`). Cycles cannot be built through the API (a replacement is always a new record); the contract rule is covered by the MV-0R fixtures. |
| M06 business time and known time | Passed | `changes.rs::m06_*`: a pin before an approval does not know it; a replacement effective in 30 days leaves the current fact in effect until then. |
| M07 contradictions | Passed | `review.rs::m07_*`: a contradicting proposal lists the conflict; accepting it puts both memories in one conflict group, both active. The review entry is the candidate list (CLI `candidates` shows the conflict count); a graphical view is MV-6. |
| M08 identity | Passed | `changes.rs::m08_*`: agents only propose; the plan shows the Markdown; each change is a new revision; every revision and its text stay readable; going back is a new reviewed revision; stale changes are refused. |
| P01 deletion across stores | Partial | `delete.rs::p01_*`: forgetting is owner-only and immediate (view, pinned reads); a purge preview lists targets, the raw export shared with other messages, and memories that lose evidence; after the purge the text is in no file of the Vault (positive control first), reads say `intentionally_purged`, verify, export, sweep, and later commits work, and the receipt says backups pending with a deadline, exports not manageable, never erased globally. Capsules, indexes, and replicas do not exist yet and are reported `not_present`; memories that lose evidence are listed, not marked `broken`; replacing a raw export with a sanitized copy is not implemented. |
| B02 old backup and deletions | Passed (logical delete) | `delete.rs::b02_*`: an export from before a forget restores with the network gate closed; reconciling the text-free ledger re-applies the deletion and only then opens the gate; a second run finds nothing to do. The purge branch of reconciliation is implemented but not separately tested. |
| Segmented catalog and scoped validation | Passed | `contract/tests/delta.rs` (equivalence with whole-set validation on every record, commit, copied and next-revision record of all synthetic sets and cases; seeded gaps are detected), `vault/tests/segments.rs` (splits and reuse over 120 commits with whole-history re-validation, group refusals, a damaged segment), and every governance test ends with whole-history validation. |

## 3. Measurements

`measure_import` (synthetic, release build, Windows 11, NTFS, SSD; the machine was noisy, so old and new binaries were run alternately):

| Case | Before (MV-2) | After (MV-3.0) |
|---|---|---|
| 4,000 imported sources, then one assertion from a fresh process | 0.71–0.78 s, 966 KB stored manifest | 0.05 s, 6 KB stored commit |
| 20,000 imported sources, then one assertion | not measured (grows with the Vault) | 0.06 s, 22 KB; 83 source segments |
| Import of 4,000 sources | 13.1–20.3 s | 14.5–15.6 s (file creation dominates) |

## 4. Checks

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 151 passed: contract 67, vault 55, import 14, govern 12, CLI 3 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `python tools/schema-check/check_schemas.py` | exit 0; 33 schemas; 53 valid records, 117 record cases, 4,759 set records, 51 IPC messages, 46 store documents agree |

## 5. Defects found and fixed

- Whole-set validation had quadratic lookups (per-review document sets, linear source and event searches); replaced by per-run indexes.
- A later import revision could change the received bytes that earlier sources cite without any rule noticing; `import.input_changed` now refuses it.
- The duplicate check compared whitespace-collapsed text, so an inserted space in Chinese text counted as a different proposal; it now ignores all whitespace.
- The store accepted canonical and governing records from any principal as long as the records themselves were valid; it now requires the Vault owner.

## 6. Not done, or pending

- Audit events for review and deletion operations are not appended yet; the review, tombstone, and receipt records are the durable account of each decision. Wiring the audit log belongs with the Host (MV-5/MV-8).
- Session-checkpoint memories (MV-5), retrieval and ranking (MV-4), the Windows review UI (MV-6).
- P01 gaps listed above; B02 purge reconciliation test.
- Small kinds are loaded completely per commit; the ADR-MEM-39 trigger (about 20,000 revisions of memories, candidates, and reviews, or a group load over 1 s) has not been measured on real data.
- Everything still pending from MV-1 and MV-2: I07 real export drill, restic, OS-crash and power-loss evidence, volume encryption detection.

Not activated: no default data root, scheduled task, model, network access, MCP, or VPS.

## 7. Commits

| Commit | Content |
|---|---|
| `ec3edae` | Scoped commit validation, property test, `import.input_changed` |
| `b2496fa` | Segmented catalog in contract and store |
| `5502017` | Candidates and plan-bound owner review |
| `64b94e3` | Supersession, identity review, owner-only store guard |
| `f6104a4` | Forget, purge, deletion ledger and reconciliation |
| `07d7435` | CLI review and deletion commands |
| (this commit) | This report and status updates |

## 8. Before MV-4

1. Owner authorization for MV-4 (index and retrieval).
2. Everything a real import still needs (I07, restic, backup media, off-device secret, real data root with the owner-only ACL).
