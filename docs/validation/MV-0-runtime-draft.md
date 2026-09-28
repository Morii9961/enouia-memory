# MV-0 validation — Memory contract freeze and repository alignment

> **Historical record (MV-0 draft).** This report was written while the MV-0 draft lived in the Enouia Runtime working tree. The independent review ([F1–F6](../reviews/MV0_REVIEW_AND_REPO_CORRECTION.md)) found contract defects, so **MV-0 is not frozen**. The draft was moved into this repository and is re-validated in MV-0R. Paths and ADR numbers below refer to the Runtime draft.

Date: 2026-09-28. Scope: MV-0 only. MV-1 has not started. The work is uncommitted in the working tree on `main`, over `cd14bcf`. The session started at `3ab5c5d`. The five Activity commits between those two points were made by another contributor; see [docs/memory/README](../memory/README.md#baseline-and-drift-check).

## Delivered

| Package | Artifacts |
|---|---|
| MV-0.1 alignment | `docs/memory/design/` (imported package, three files redacted), [docs/memory/README](../memory/README.md) (baseline, drift, import hashes, milestone mapping), [ADR-020–029 with ADR-MEM mapping](../adr/memory-extension-v1.md), register entries in [docs/adr/README](../adr/README.md), [contract change note](../memory/CONTRACT_CHANGES_MV0.md), pointers in README, IMPLEMENTATION_PLAN_v0.3 and CONTRACT_BOUNDARIES_M0 |
| MV-0.2 contracts and fixtures | 15 schemas in `contracts/memory`, 4 in `contracts/context`, 1 in `contracts/provider`, `contracts/ipc/memory-v1.schema.json`. Crate `enouia-memory-contract` (22 modules): typed IDs/time/hashes, records, strict parsing, per-record and cross-record validators, temporal and egress rules, IPC types. Fixtures in `tests/fixtures/memory`: 43 valid records (all 18 record kinds, all five memory types), 90 invalid record mutations, 3 consistent sets (MoriMeta confirmed, MoriMeta without sufficient evidence, lifecycle), 46 set mutations, 12 valid requests, 10 valid responses, 29 invalid IPC messages |
| MV-0.3 ports/errors/dependencies | `ports` module (IdSource, VaultReader/Writer, CommitRequest/Outcome, AuditSink, PolicyGate, ProviderPort, SecretStore, BackupPort; pure `classify_retry`, `commit_time`), `MemoryErrorCode` (23), `layout` (relative Vault paths), [constraint map](../memory/CONTRACT_CONSTRAINTS_MV0.md) |
| MV-0.4 checks | This report |

## Commands and results

Environment: Windows 11 Home 10.0.26200. C: and E: are both local NTFS. Rust/Cargo 1.98.1 (`stable-x86_64-pc-windows-gnu`, matching the pin), with `CARGO_NET_OFFLINE=true` and `RUSTUP_TOOLCHAIN=stable-x86_64-pc-windows-gnu`, run from the workspace root. No new external crates; Cargo.lock only gains the new workspace package.

| Command | Result |
|---|---|
| Baseline at `3ab5c5d`, before any change: `cargo fmt --all -- --check`; `cargo test --workspace` | exit 0; exit 0 |
| `cargo fmt --all -- --check` | exit 0 |
| `cargo test --workspace` | exit 0; 166 passed, 0 failed. The new crate contributes 35: 12 unit, 9 conformance, 6 record-set, 4 IPC, 4 boundary |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| `cargo tree -p enouia-memory-contract --offline` | only `enouia-common`, `serde` 1.0.229 and `serde_json` 1.0.151 (plus their locked transitive crates) |
| `cargo tree --workspace -i enouia-memory-contract` | no dependents: no Activity crate uses it |

One intermediate full run failed two `enouia-windows-process` Cowork discovery tests. Their sandboxes are `target/cowork-discovery-tests/<pid>_<n>` and are never cleaned; 144 leftovers were present, so a reused PID lands in a populated directory. Three reruns of that suite and the final full run passed. This test-isolation flaw predates MV-0, sits in code MV-0 did not touch, and is reported separately.

## Acceptance D01–D04: contract/static parts

| ID | MV-0 evidence | Status |
|---|---|---|
| D01 Schema positives/negatives and field round-trip | Every valid record passes both the schema harness and the Rust parser, and round-trips to an identical JSON value. The canonical bytes equal the stored fixture bytes. All 90 invalid mutations are rejected by Rust under the named rule. 58 of them are also rejected by the schema, and 32 are marked `schema: accept`: constraints JSON Schema cannot express, such as real dates, time order, locator safety and secrets. 46 set mutations produce their named cross-record violation. IPC: all 12 operations, request/response pairing, and 29 invalid messages | **Passed (contract/static)** |
| D02 Major version and migration | An unknown major is classified `UnknownMajorReadOnly`: `ensure_writable` refuses it and parsing returns `unsupported_schema`. A missing version is `Malformed`. Schema `const 1` | **Static part passed.** Keeping the old CURRENT after a failed migration and restoring from a pre-upgrade snapshot: **pending MV-1** |
| D03 Dependencies and data root | Manifest dependencies are exactly common/serde/serde_json. No Activity ↔ Memory-contract dependency in either direction. The contract source has no `std::fs`/`net`/`process`/`env`, `include_str!`, `SystemTime` or Moriium references. Confirmed by `cargo tree` | **Static part passed.** "Remove both checkouts and run the installed artifact": **pending MV-1** (no artifact exists) |
| D04 Domain isolation | Every Vault layout path sits under `vault/`, `indexes/`, `config/`, `backup-state/` or `sync-state/`, never `activity/` | **Static part passed.** Activity hash/sequence unchanged across Memory import/delete/rebuild/restore, and Activity failure not blocking the Vault: **pending MV-1** |

## What is synthetic or static only

- All records, sets and IPC messages are synthetic. The MoriMeta story exists as test data, in two variants: with the user's confirmation, and without sufficient evidence. It is not a real memory, and nothing was imported.
- The schema harness is an in-repository subset validator with its own small regex engine, because no JSON Schema crate was in the offline cache. It fails on any unsupported keyword and is covered by self-tests. It has not been cross-checked against an independent implementation such as ajv or python-jsonschema. That cross-check is recommended once a vetted tool is available.
- Fixture capsules, inspections and dispatches are hand-built. The tests prove they agree with the frozen hard rules (tombstones, policy, provenance, valid time, conflict visibility, currency labels, capsule ⊇ dispatch). They do not prove that a compiler selects them; that is MV-5.
- Commit-catalog `content_hash` values are computed over canonical bytes by the generator, but no Rust test recomputes them yet (MV-1).
- 126 of the 219 validator rule IDs have a dedicated negative fixture. The rest are implemented without their own fixture; the [constraint map](../memory/CONTRACT_CONSTRAINTS_MV0.md#rule-coverage-by-negative-fixtures) lists them.

## Not done; left to MV-1 or later

- **Transactions and recovery**: atomic `CURRENT` publication, a staging/manifest/CURRENT fault-injection matrix (V02), lost-response replay from a durable receipt (V03), a multi-record review transaction (V04), disk full or sharing violations (V05), corrupt CURRENT or objects entering `vault_recovering` (V06), path and reparse safety (V07), pinned reads during commits (V08), the two-process writer lock (V01).
- **Durability**: process-crash recovery, OS crash, and power loss (V09) need separate evidence. No power-loss claim is made. MV-1 must say which level each test proves.
- **Backup**: restic adapter, restore to a new location without the original DPAPI, and restore key custody (B01–B04). Backup media and destination are user decisions, needed before MV-1.4 activation. They do not block MV-1.1–1.3 development.
- **Scale**: manifest write amplification with a full catalog, and the 1 GiB / 100k-message benchmarks (Q02 context).
- Not done by design: data root creation, real import, model calls, UI, MCP, VPS, scheduled tasks, publication.

## MV-0.4 test plan inputs for MV-1

| Item | Current fact | MV-1 need |
|---|---|---|
| Windows version / filesystem | Windows 11 Home 26200; NTFS on C: and E: | Test same-volume replace and flush on NTFS; record the build. Reject network, OneDrive and reparse roots (V07) |
| Crash vs power-loss evidence | None | Separate process-kill, OS-restart and (optional) power-loss experiments; label each result with its level |
| Default data root | `%LOCALAPPDATA%\EnouiaRuntime` (not created) | Tests use isolated temporary roots only |
| Backup destination / restic version | Not chosen, not installed | The user chooses media before the backup activation gate; pin the version and verify its source |
| Scale fixture | Not built | A synthetic generator for large vaults; measure catalog size and commit latency |

## Rollback

Everything MV-0 added is uncommitted: new directories `contracts/{memory,context,provider}`, `contracts/ipc/memory-v1.schema.json`, `crates/enouia-memory-contract`, `docs/memory`, `docs/adr/memory-extension-v1.md`, this file, and `tests/fixtures/memory`. Additive edits touch `Cargo.toml` (one workspace member), `Cargo.lock` (one package entry), `README.md`, `docs/adr/README.md`, `docs/CONTRACT_BOUNDARIES_M0.md` and `docs/IMPLEMENTATION_PLAN_v0.3.md`. To roll back, delete the new paths and revert those six files. No Activity file, data root, credential or remote state was touched.
