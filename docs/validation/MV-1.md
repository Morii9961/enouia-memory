# MV-1 — Vault and recovery foundation

Date: 2026-09-28. Scope: MV-1 only ([plan §5](../design/IMPLEMENTATION_PLAN.md)), authorized by the owner after MV-0R. Start: `729edc9` (MV-0R report). MV-2 has not started. Everything below uses synthetic data in temporary directories; no real memory, chat, export, or data root was read or created.

## 1. What exists now

| Work package | Delivered | Code |
|---|---|---|
| MV-1.0 store contracts | Vault descriptor, `CURRENT` pointer (pins the manifest SHA-256), publish journal line, idempotency entry, owner recovery receipt, pinned-commit export manifest, restore state. JSON Schemas, Rust types, 7 valid and 23 invalid fixtures, independent Python cross-check (ADR-MEM-36) | `contracts/store/`, `src/store.rs` (contract crate), `tests/fixtures/store/` |
| MV-1.1 storage safety | Data root verification, managed paths with reparse checks, OS single-writer lock, write-through atomic replace, CSPRNG IDs, owner-only protected DACL entry point and inspection, free-space floor | `enouia-memory-vault`: `root.rs`, `fs.rs`, `lock.rs`, `platform/` |
| MV-1.2 commit and recovery | Immutable revisions, complete manifest, one `CURRENT` publication, expected head/revisions, idempotent receipts (replay/conflict), pinned verified reads, full-history cross-record validation before publication, integrity report, freshness check before content leaves, owner-driven recovery with evidence, failure injection at every boundary | `store.rs`, `fault.rs` |
| MV-1.3 minimal objects | Manual assertion sources; sessions with user messages saved together with their exact text; health (`Healthy/Degraded/Unavailable/Recovering`); restricted, size-rotated audit | `service.rs`, `health.rs`, `audit.rs` |
| MV-1.4 backup exit | Pinned-commit export, independent export verification, restore into an empty verified root with a network gate, restic command adapter | `backup.rs`, `restic.rs` |
| Local entry point | `enouia-memory` CLI: check-root, init, status, verify, recovery, adopt, assert, export, verify-export, restore, acl, protect | `enouia-memory-cli` |
| Measurement | Manifest write amplification and commit latency | `examples/measure.rs` |

Design decisions are recorded in ADR-MEM-36 (file contracts, port refinements) and ADR-MEM-37 (store implementation, measured cost and trigger) in the [ADR register](../adr/README.md).

## 2. Acceptance items

"Passed" means an automated test in this repository demonstrates the behavior on this machine. "Partial" and "Pending" say exactly what is missing.

| Item | Status | Evidence (test file :: test) |
|---|---|---|
| V01 two writers | Passed | `writer_lock.rs`: same-process second writer gets `busy`; a writer in another process blocks until released; a crashed holder never leaves a stale lock; two processes racing one request produce exactly one commit and one replay |
| V02 every persistence boundary | Passed (process crash) | `crash.rs`: a child process is aborted at all 11 boundaries (lock, staging, records, objects, manifest, idempotency, before `CURRENT`, inside the first file write, before the `CURRENT` rename, after `CURRENT`, after the journal). Readers see the whole old or the whole new commit; unpublished files never become records; staging is cleared |
| V03 lost response | Passed | `crash.rs`: after `CURRENT` the retry replays the same receipt; a different payload under the same key is `idempotency_conflict`; `failures.rs`: an error reported after publication also replays |
| V04 accept + supersede atomically | Passed | `crash.rs` uses the lifecycle commit that writes a candidate revision, a review, a new memory, and the superseded memory together; `commit.rs` replays all 19 lifecycle commits with full cross-record validation at each step |
| V05 disk full / access denied / sharing | Passed (injected) | `failures.rs`: errors 112, 5, and 32 injected at seven points keep the old head, name the fault (`storage_full`, `storage_failed`/access denied, `busy`/sharing violation), and the retry applies once. The volume was not actually filled or locked |
| V06 damaged `CURRENT` / object | Passed | `failures.rs`: torn `CURRENT` and a damaged head manifest give `vault_recovering`; the report lists journal evidence first and marks the damaged commit incomplete; only the owner can adopt a complete point, with a receipt; an unpublished complete commit is listed but never adopted by itself; a damaged record or object is reported by exact reference and refused (non-retryable) while other records stay readable |
| V07 escape and unverified roots | Passed | `root.rs`: relative, UNC, device, verbatim-UNC, missing, file, non-canonical, junction (root or ancestor), sync-folder names, policy sync roots, Git trees (including this repository), Program Files, low space, and unsupported filesystem are refused; a junction planted inside the Vault cannot redirect a commit and nothing is written through it |
| V08 reads during commits | Passed | `snapshot.rs`: a reader verifying its pinned commit in a loop while 17 commits land always sees a whole commit; a deletion committed after the pin is refused (non-disclosing `not_found`) before content leaves, while the old pin stays a consistent snapshot; a policy change after the pin is reported for re-decision |
| V09 reliability level | Partial | Only process termination is evidenced (V01/V02/V03). No OS crash, forced reboot, or power-loss experiment was run. **No power-loss or OS-crash durability is claimed.** Write-through flags and flushes are used, but their effect on real hardware is unverified |
| D02 migration | Passed | `migration.rs`: an unknown-major record is refused; a descriptor with a newer layout opens as `unsupported_schema` and is left untouched; a failed migration keeps the old `CURRENT`; a successful one is undone by restoring the pre-upgrade export |
| D03 no other checkout | Passed | `cli.rs`: the built binary, copied out of the repository and run with a cleared environment (System32 only on `PATH`), performs the full lifecycle; a fresh clone in an isolated temporary directory with its own target directory passes every check (§4) |
| D04 Activity isolation | Passed | `backup.rs`: an Activity-like tree beside the data root is byte-identical after genesis, 19 commits, export, restore, recovery report, and health; managed paths reject `activity/…`. Memory has no Activity dependency (`boundaries.rs`) |
| B01 independent encrypted backup | Partial | The plaintext half passes (`backup.rs`): after the original root is deleted, a pinned export restores elsewhere with identical manifests and record bytes, rebuilt idempotency (the last request replays, the next commits and matches the fixture), and network disabled until reconciliation. **Pending:** restic is not installed, so no encrypted repository, `check`, or restore from it has run; no second machine or account; DPAPI independence of the backup secret is untested |
| B03 bad snapshot / key loss | Partial | Passed: flipped, missing, unlisted, and edited exports are refused before the target is touched; a restore never overwrites a healthy Vault. **Pending:** key rotation and key loss need restic |
| B04 local protection and logs | Partial | Passed: the owner-only protected DACL entry point (user, SYSTEM, Administrators; children inherit; no broad grants), from code and from the CLI; a sentinel written as record text appears only in the source record and the session-content object, never in manifests, `CURRENT`, journal, idempotency entries, audit lines, health, errors, or CLI output. **Pending:** volume encryption (BitLocker) status is not detected (it needs elevated tools); the ACL was applied only to temporary directories |

## 3. Measurements

Environment: Windows 11 Home 10.0.26200, NTFS on the system SSD, Rust 1.98.1 `x86_64-pc-windows-gnu`, release build, one synthetic manual assertion per commit (`cargo run --release -p enouia-memory-vault --example measure -- 2000`). One run; figures are indicative, not a benchmark.

| Records in catalog | Head manifest bytes | Cumulative manifest bytes | Avg ms/commit, no set validation | Avg ms/commit, full validation |
|---|---|---|---|---|
| 251 | 61,630 | 7.9 MB | 58.5 | 44.0 |
| 501 | 121,880 | 30.9 MB | 65.6 | 328.3 |
| 751 | 182,130 | 68.9 MB | 51.8 | 571.2 |
| 1,001 | 242,381 | 122.0 MB | 62.0 | 420.6 |
| 2,001 | 483,381 | 485.0 MB | 51.7 | – |

Reading: each record costs about 241 bytes in every manifest, so manifests grow linearly and their total grows quadratically with single-record commits. Latency without validation is dominated by flushes and write-through renames (roughly 50–150 ms). Full-history validation grows with the Vault. ADR-MEM-37 sets the trigger: a segmented or incremental catalog and incremental validation are required before a head manifest exceeds 1 MiB (about 4,000 records) or a commit exceeds 1 s. Importing real history (MV-2) will cross this quickly unless imports batch many records per commit, which they will.

## 4. Checks

| Command (repository root) | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace --locked` | exit 0; 109 passed: contract 58 (16 unit, 6 boundaries, 9 conformance, 4 IPC, 7 record sets, 14 regressions, 2 store contracts), vault 49 (4 unit, 6 commit, 6 backup, 5 crash, 7 failures, 3 migration, 5 objects, 5 root, 3 snapshot, 5 writer lock), CLI 2 |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | exit 0 |
| `cargo test --release --locked -p enouia-memory-contract --test regressions` | exit 0 |
| `python tools/schema-check/check_schemas.py` | exit 0; 30 schemas; 51 valid records, 108 record cases, 4,253 set records, 51 IPC messages, 30 store documents agree |

Isolated build: `5da02eb` was cloned from GitHub into a temporary directory with a fresh `CARGO_TARGET_DIR` and no sibling checkout; fmt, all 109 tests, clippy, release regressions, and the schema cross-check passed. The clone contains no `.local/` and no `docs/history/private/`.

## 5. Defects found and fixed during MV-1

- **Retry blocked after a pre-publication crash** (found by the crash matrix): the manifest left under the caller's commit ID made the retry fail as a reused ID. Only a manifest on the published chain is now a reuse; an unpublished one is quarantined.
- **Identity revision without its Markdown**: the synthetic lifecycle cataloged an identity whose content object was never stored. The fixture now lists that object, and the store requires it (`store.identity_markdown_missing`).
- **Export flush** failed with access denied on Windows because `FlushFileBuffers` needs a writable handle.
- **Canonical catalog order** is by record-kind name, then ID, matching the frozen fixtures.

## 6. Not done, or pending

- Power-loss and OS-crash evidence (V09); real disk-full and real sharing-violation reproduction (V05 used injection).
- restic: installing a pinned, verified version, the owner's choice of backup media and repository, the password kept outside this device, encrypted backup/check/restore, key rotation and loss, and restore on another machine or account (B01, B03). No download was made.
- Volume encryption detection; applying the ACL to a real data root (B04).
- Sweeping unreferenced revision files left by crashed attempts (they are never read and are quarantined when a later write needs their name); backup leases and GC arrive with purge (MV-3).
- The segmented catalog and incremental validation (§3 trigger).
- `AuditSink` exists and is tested, but reads are not yet audited because there are no read APIs for other principals yet (MV-4/5). `BackupPort` is not wired to restic yet.
- Cloud-sync detection is attribute- and name-based; a sync client configured on an arbitrarily named folder without cloud-file attributes would not be recognized. Real OneDrive folders were not touched.

Not activated: no default data root, scheduled task, backup job, model, MCP, VPS, network, or sync.

## 7. Commits

| Commit | Content |
|---|---|
| `9151b78` | MV-1.0 store file contracts (ADR-MEM-36) |
| `c46907f` | Store: verified roots, writer lock, atomic commits, lifecycle replay |
| `4cac344` | V01–V08 tests; fix for the retry after a pre-publication crash |
| `13b60c7` | MV-1.3 sources, sessions, health, audit |
| `33ba082` | MV-1.4 export, verification, restore, restic adapter |
| `6e21edd` | CLI; D02 and D03 |
| `5da02eb` | Measurement example |
| (this commit) | ADR-MEM-37, documentation, this report |

Every push was preceded by a public-content scan of the staged files. Rollback: each step is a separate commit on `main`; `729edc9` is the MV-0R state.

## 8. MV-2 entry and inputs needed

MV-2 (history import and rescue) builds the importer on this store. Before it starts, the owner decides:

1. Explicit authorization for MV-2.
2. Whether real exports may be used at all in MV-2 and which ones; until then MV-2 runs on synthetic exports only.
3. Before any real import: install a pinned restic version, choose backup media, keep the restore secret off this device, and run the B01/B03 restore drill on the synthetic Vault.
4. Where the real data root will live (the default `%LOCALAPPDATA%\EnouiaMemory` or another verified local path), and approval to apply the owner-only ACL there.
5. The segmented catalog should land early in MV-2 if imports produce thousands of records (ADR-MEM-37 trigger).
