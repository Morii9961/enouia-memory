# Contract notes (MV-0R, updated in MV-1)

Status 2026-09-28: these are the corrected MV-0 contracts after the [independent review](../reviews/MV0_REVIEW_AND_REPO_CORRECTION.md). They are kept as a contract freeze candidate for MV-1 to build on. Behavior that needs a real store (atomic commits, recovery, durability) is **not** proven by these contracts; it is MV-1 evidence ([MV-1 report](../validation/MV-1.md)). MV-1 added the store file contracts (`contracts/store/`, ADR-MEM-36) and refined three ports, noted below.

## 1. Relation to Enouia Runtime M0

Runtime's M0 agreement fixed the Memory record's minimum shape. Everything below keeps it and adds to it:

| Runtime v0.3 / M0 statement | Memory contract |
|---|---|
| Required memory fields `schema_version`, `memory_id`, `type`, `content`, `source_id`, `created_at`, `updated_at`, `status (active\|superseded\|archived)` | Unchanged. New fields are required keys with an explicit `null` for unknown. Deletion is a tombstone, never a status |
| ProjectState `project_id`, `state`, `decisions[]`, `open_loops[]` | Same top-level names. Items carry evidence and a `state_kind`; decisions must be `decided` |
| Checkpoint with covered turns, `last_state`, `open_loops[]` | Adds checkpoint ID and revision, branch, `covered_events`, `coverage_hash`, `last_completed_turn_id`. The referenced artifact must be reviewed |
| "Explicit remember/inspector saves may commit" | Only through an owner review on a trusted surface backed by a `manual_assertion` (ADR-MEM-22) |
| "Stage, validate, flush, atomically replace, then update SQLite" | Commit = immutable revisions + a complete catalog + one `CURRENT` (ADR-MEM-21) |
| v0.2 capsule fields | All kept. Adds version, `requested_by`, `as_of`, commit and epochs, versions, surface, destination, purpose, currency per item, `verification_needed`, `completeness` |
| Future MCP `memory_update` | Alias of `memory_propose` (revise/supersede only); always returns a pending candidate |
| Shared `enouia-common` ports and `ErrorCode` | Not used. Memory owns `foundation` ports, `ComponentId`, and `MemoryErrorCode` (ADR-MEM-19/20) |

## 2. Key decisions

- **IDs**: `<prefix>_<lowercase UUID v4>`, 31 namespaces (`ids::ID_PREFIXES`, mirrored in `contracts/memory/common-v1`), including `apv` (approval).
- **Time**: stored timestamps have the form `YYYY-MM-DDTHH:MM:SS.mmmZ` and must be real dates. `BusinessTime` is either a timestamp or `"unknown"`. Unknown stays unknown (ADR-MEM-32).
- **Numbers**: every integer lies within ±(2^53 − 1), enforced in Rust before typed parsing. Arithmetic is checked (ADR-MEM-34).
- **Versions**: `schema_version`, `revision` and `commit_id`/`sequence` are separate. An unknown major version is read-only: its bytes are kept and never rewritten.
- **Unknown fields**: strict. Future non-security fields go in dotted-namespace `extensions`, preserved verbatim; `enouia.*` is reserved. Security-relevant records (review, approval, policy, commit, tombstone, capsule, dispatch) have no extensions.
- **Canonical bytes**: UTF-8, sorted keys, 2-space indent, LF, trailing LF. Commit catalogs and grant/approval digests hash these bytes.
- **Approvals** (ADR-MEM-30): egress, declassification and policy-grant approvals are bound to exactly what they approve. A memory review can never stand in for one of them.
- **Policies** (ADR-MEM-31): versioned, default deny, project and Provider scoped, revocable. The genesis default never sends outside.
- **Payload digest** (ADR-MEM-33): destination, messages, tools and output configuration are bound together. Any change needs new consent.
- **Deletion**: an owner `delete` proposal, then a `confirm_delete` review with `delete_binding`, then a tombstone that must equal the binding.
- **Mock budget**: `utf8_bytes_v1`, deterministic but not an upper bound for real tokenizers.

## 3. Ports and error semantics

Host ports (`foundation`): `Clock`/`FakeClock`, `Cancellation`, `WriterLock` (OS process-exclusive; an in-process mutex does not count), `AtomicFile` (a single-file primitive only), `ComponentId`. Domain ports (`ports`):

| Port | Contract |
|---|---|
| `IdSource` | 16 random bytes → typed v4 ID. `SequentialIdSource` for tests; `OsIdSource` (BCryptGenRandom) in `enouia-memory-vault` |
| `VaultReader` | Pins a commit with its epochs. An unverifiable `CURRENT` gives `vault_recovering`, never a guess |
| `VaultWriter` | One transaction per call under the writer lock. The caller pre-assigns `commit_id` (reviews name their commit). Checks `expected_commit_id` and per-record expected revisions (`revision_conflict`). The idempotency scope (required) is principal + commit `OperationKind` + key hash. `StagedObject` carries its `ObjectKind`. Returns `Committed` or `Replayed` |
| `classify_retry` | Same scope + same payload → replay. Different payload → `idempotency_conflict` |
| `commit_time` | A clock earlier than the parent commit gives `clock_regression` |
| `AuditSink` | An audit failure closes external reads and dispatch (`audit_unavailable`) |
| `PolicyGate` | `AccessRequest` = authenticated principal, operation, scope, purpose, full destination, server-resolved targets (record revision, project, sensitivity). Default deny; `policy::evaluate` is the reference |
| `ProviderPort` | Receives only a `ProviderRequest` that passes `verify_against` for its Dispatch |
| `SecretStore` / `SecretBytes` | Not serializable, not clonable, redacted Debug |
| `BackupPort` | Backs up a pinned commit under a lease. A restore plan keeps the network off until tombstones and revocations are reconciled. MV-1 implements the export/verify/restore functions and a restic command adapter; the trait itself is wired once restic is installed (lease/GC arrive with purge in MV-3) |

`MemoryErrorCode` has 23 codes. `unauthenticated`, `permission_denied` and `not_found` are non-disclosing. IPC errors carry only `{code, component, retryable}`.

## 4. Dependency direction and data

```text
enouia-memory-contract  (serde, serde_json only)
        ↑
enouia-memory-vault     (+ windows-sys =0.61.2 on Windows)   ← MV-1
        ↑
enouia-memory-cli       (local entry point)                   ← MV-1
(future) context / session / provider crates in this repository
        ↑
Enouia Runtime adapters (Windows client), via versioned contracts or a pinned release
```

Nothing here reads Activity data or depends on Runtime, Moriium or credentials. `tests/boundaries.rs` enforces this: no path outside the repository, no Git dependency, no `std::fs`/`net`/`process`/`env` in the contract crate. Everything needed for recovery is a file record under `vault/`: reviews, approvals, policies, idempotency receipts, tombstones, checkpoints and session events.
