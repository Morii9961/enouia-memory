# MV-0R — independent repository move and contract correction

Date: 2026-09-28. Scope: MV-0R only ([task](../handoff/CLAUDE_MV0_CORRECTION_PROMPT.md), [review](../reviews/MV0_REVIEW_AND_REPO_CORRECTION.md)). MV-1 has not started. The earlier MV-0 report is kept as history ([MV-0 draft report](../history/MV-0-runtime-draft-report.md)); its "35 tests passed" is **not** used as evidence that MV-0 was frozen.

## 1. Migration: inventory and hashes

| Step | Evidence |
|---|---|
| R0 source state | Runtime `main` at `cd14bcf` (equal to `origin/main`). 121 untracked Memory files + 6 modified shared files. The SHA-256 of all 127, the exact shared-file patch, and full copies are kept in the ignored local snapshot `.local/mv0r-snapshot/`. Copies were verified against the manifest (121 + 6) and the patch reverse-applied cleanly |
| Documents | The 16 root documents were copied to `.local/originals/` with SHA-256, then moved under `docs/`. 9 design files are byte-identical. 6 had only machine paths redacted and links repaired (listed in [docs/README](../README.md)). The personal v0.1 draft is unchanged (`bfc7d55d…`) and stays in the ignored `docs/history/private/` |
| Copy into this repository | 108 files copied from the snapshot and verified byte-identical against the R0 manifest. 13 were deliberately not copied: Runtime's redacted copy of the design package (12) and its Runtime-oriented index (1) |
| Decoupling edits | 8 files changed after the verified copy: `Cargo.toml` (crate), `src/error.rs`, `src/lib.rs`, `src/ports.rs`, `tests/boundaries.rs`, `tests/support/schema.rs`, `contracts/memory/common-v1.schema.json`, `contracts/ipc/memory-v1.schema.json`. Regenerated fixtures were byte-identical to the snapshot at that point |
| Remote | `Morii9961/enouia-memory`: public; `ls-remote` showed no refs before the first push. It was not recreated or force-pushed |

The runtime data root recommendation is now `%LOCALAPPDATA%\EnouiaMemory` (not created). No real data was read, moved, or created.

## 2. Findings F1–F6: fixes and tests

| Finding | Fix (ADR) | Location | Regression evidence |
|---|---|---|---|
| F1 egress consent not bound | `ApprovalRecord` egress binding: request, capsule, payload digest, destination, resource revisions, policy epoch, TTL ≤ 15 min, single use. Dispatch `resource_refs` covers memory, Identity, checkpoint, event, source, attachment (ADR-MEM-30, 25) | `src/approval.rs`, `src/context.rs`, `src/set.rs::check_dispatch` | `regressions.rs::f1_*`. Record cases `dispatch-review-as-egress-approval`, `approval-*`. Set cases `egress-approval-{missing,wrong-kind,wrong-request,wrong-payload,wrong-provider,expired,replayed,wrong-resources,wrong-epoch}`, `dispatch-hidden-attachment`. A valid external set proves the positive path |
| F2 delete / declassification not bound | Declassification approval bound to the exact revision, both levels, sources, and content. `confirm_delete` `delete_binding` must equal the tombstone. Approval fields are typed `ApprovalId` (ADR-MEM-30) | `src/candidate.rs`, `src/set.rs::{check_evidence,check_deletions}` | `regressions.rs::f2_*`. Set cases `tombstone-{ordinary-accept-review,binding-mode-mismatch,wrong-target}`, `declassification-{missing,wrong-revision,wrong-level,content-changed}`. Record cases `memory-review-as-declassification`, `review-confirm-delete-without-binding` |
| F3 same-length payload accepted | `request_hash` = SHA-256 of the canonical payload (destination, messages, tools, output). `ProviderRequest::verify_against` replaces `matches_dispatch`. Pure SHA-256 checked against FIPS 180-4 vectors (ADR-MEM-33) | `src/context.rs`, `src/provider.rs`, `src/hash.rs` | `regressions.rs::f3_*`: same-length swap, reordering, tool definition, output, destination. Record cases `dispatch-{request-hash-mismatch,same-length-content-swap,output-changed}` |
| F4 policy input too thin | Persistent `PolicyRecord` (versioned, revocable, genesis default and owner grants bound to an approval) and default-deny `policy::evaluate` over principal, scope, project, kind, sensitivity, purpose, provider/model. `PolicyGate` takes server-resolved targets and the full destination (ADR-MEM-31) | `src/policy.rs`, `src/ports.rs`, `src/set.rs::{check_policies,hard_exclusion}` | `regressions.rs::f4_same_principal_and_sensitivity_differ_by_project_and_provider`. Set cases `egress-{grant-revoked,grant-tampered,other-provider,other-project-memory}` |
| F5 unknown start became approval time | Unknown stays unknown; both sides `needs_reverification`; each edge must equal the review's `effective_from` (ADR-MEM-32) | `src/temporal.rs`, `src/set.rs` | `regressions.rs::f5_*` (approved 09-21, queried 09-25 / 09-28 / 2030) |
| F6 panic on extreme numbers | ±(2^53−1) integer check before typed parsing; checked budget sum; checkpoint coverage no longer iterates a caller-supplied range (ADR-MEM-34) | `src/json.rs`, `src/record.rs`, `src/ipc.rs`, `src/context.rs`, `src/set.rs` | `regressions.rs::f6_*`. Record cases `capsule-budget-*`, `event-sequence-u64`. Set case `checkpoint-huge-range`. Release build run |

Each finding's code, schemas, fixtures, ADR, and constraint map were updated together ([ADR register](../adr/README.md), [contract notes](../contracts/CONTRACT_NOTES.md), [constraint map](../contracts/CONSTRAINT_MAP.md)).

## 3. Checks and results

Environment: Windows 11 Home 10.0.26200 (NTFS), Rust/Cargo 1.98.1 `x86_64-pc-windows-gnu`, `CARGO_NET_OFFLINE=true`, Python 3.12.10 venv with the pinned `tools/schema-check/requirements.txt` (jsonschema 4.26.0, referencing 0.37.0, rpds-py 2026.6.3).

| Command (from repository root) | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 52 passed: 14 unit, 4 boundaries, 9 conformance, 4 IPC, 7 record sets, 14 regressions |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --release --locked --test regressions` | exit 0 (F6 in release mode as well) |
| `python tools/schema-check/check_schemas.py` | exit 0; 23 schemas valid against the 2020-12 metaschema. 51 valid records, 108 record cases, 4253 records inside sets and set mutations, and 51 IPC messages all agree with the manifest expectations |

Independent build (step 6): the pushed repository was cloned into a temporary directory on another drive, with no Runtime checkout beside it and a fresh `CARGO_TARGET_DIR`. The results were the same: fmt 0, 51 tests (at `2a8b842`, before the catalog-digest test was added), clippy 0, release regressions 0, schema check OK. No manifest has a path outside the repository or a Git dependency (`tests/boundaries.rs`). The Runtime directory was not renamed or deleted to prove this.

What is synthetic or static only: every fixture is synthetic, and no real Provider, Vault, or data was used. Capsules and Dispatches are hand-built fixtures; the tests prove they obey the frozen rules, not that a compiler produces them (MV-5). 156 of the 265 validator rules have a named negative case.

## 4. Enouia Runtime cleanup

Before cleaning, Runtime was re-read: still `cd14bcf` = `origin/main`, and the untracked set, file contents, and shared-file diff were identical to R0. Nothing was mixed in. Cleanup removed exactly the 121 snapshot-listed untracked files (plus the empty directories they left) and reverse-applied the exact 6-file patch. No `reset --hard`, no `git clean`, no whole-file revert. Afterwards `git status` is empty. Runtime's own checks pass: fmt 0, `cargo test --workspace` 131 passed (166 minus the 35 Memory tests that moved), clippy 0.

Other contributors' work is preserved: the five Activity commits `53644c3…cd14bcf` (pause, retry record/gate, delivery overview) sit in committed history, which the cleanup never touched. Ignored build outputs of the old draft may remain under Runtime's `target/`; they are untracked build products. The existing Cowork test-sandbox flakiness in Runtime was reported separately and not changed here.

## 5. Commits in this repository

| Commit | Content |
|---|---|
| `da0d1e3` | Documents organized under `docs/` |
| `ffe313b` | MV-0 draft imported as an independent workspace (explicitly not frozen) |
| `b7b9cd9` | Absolute schema `$id`s + pinned independent validator |
| `b1c2b7f` | F3 + F6 |
| `a7e0e29` | F5 |
| `2a8b842` | F1 + F2 + F4 |
| (this commit) | Catalog-digest test, ADR register, contract notes, design v1.1 corrections, this report |

All commits use the GitHub noreply identity. Every push was preceded by a public-content scan of the staged files: no machine paths, personal data, or credential shapes beyond allowlisted synthetic sentinels. Rollback: every step is a separate commit on `main`, and the Runtime state can be restored from `.local/mv0r-snapshot/` (`shared-files.patch` plus `untracked/`).

## 6. Not done, or pending for MV-1 and later

- Transactions and recovery: atomic `CURRENT` publication, the crash-boundary matrix, lost-response replay, the two-process writer lock, disk full, corrupt `CURRENT`/objects, path/reparse safety (V01–V08). Durability: process crash vs OS crash vs power loss must be evidenced separately (V09). **No durability claim is made.**
- D02–D04 behavior: failed migration keeps the old `CURRENT`; the installed artifact runs without any other checkout; Activity data is unchanged across Memory operations.
- Approval issuance: nonce generation and expiry on a trusted surface, and the diff-hash recomputation UI (MV-3/MV-6). Real `PolicyGate`/`ProviderPort` wiring (MV-5/MV-7). Storage, integrity, and deletion of the private request bodies addressed by `content_hash` (MV-1/MV-5).
- Backup/restore (B01–B04), import (I01–I07), retrieval (R01–R06), compiler (C01–C08). Nothing is activated: no data root, scheduled task, model, MCP, VPS, or sync.

## 7. MV-1 entry and prerequisites

Entry: [design/IMPLEMENTATION_PLAN §5](../design/IMPLEMENTATION_PLAN.md), implemented in this repository by adding a storage crate that implements `ports::{VaultReader, VaultWriter}` and `foundation::{WriterLock, AtomicFile}` against isolated temporary roots only. Targets: V01–V09, D02–D04 behavior, B01, B03, B04. Prerequisites:

1. The owner explicitly authorizes MV-1.
2. Tests use temporary roots only; `%LOCALAPPDATA%\EnouiaMemory` is not created until a later, explicit step.
3. Before MV-1.4 backup activation, the owner chooses backup media and keeps the restore secret.
4. Runtime integration (a Runtime-side ADR and adapter consuming a pinned version of this repository) is scheduled separately, in Runtime.
