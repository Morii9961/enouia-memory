# MV-0 contract change note

> **Draft (pre-MV-0R).** Written against the Runtime draft; corrected in MV-0R.

Date: 2026-09-28. This note records what MV-0 adds to or tightens in Architecture v0.3 and the [M0 agreement](../CONTRACT_BOUNDARIES_M0.md). Every existing M0 field and enum is kept. Nothing in ActivityData v1, batch v1, `contracts/ipc/common-v1` or `contracts/ipc/activity-v1` changed.

## 1. Differences from v0.3 / M0

| v0.3 / M0 statement | MV-0 contract | ADR |
|---|---|---|
| Required memory fields: `schema_version`, `memory_id`, `type`, `content`, `source_id`, `created_at`, `updated_at`, `status (active\|superseded\|archived)` | Unchanged. New fields are required keys whose unknown values are explicit `null`. The status enum is unchanged. Deletion is a tombstone, never a status | 020, 026 |
| Optional `project_id`, tags, validity, confidence, supersession | Now required-nullable. Adds `evidence[]` (≥1), `epistemic_status`, `volatility`, `priority`, `sensitivity`, access and egress policy IDs, `provenance_state`, `review_id`/`approved_by`/`approved_at`, `declassification_review_id`, `extensions` | 022, 023 |
| ProjectState: `project_id`, `state`, `decisions[]`, `open_loops[]` | Same top-level names. `state` and `decisions` are arrays of evidenced items with `state_kind` (planned/decided/implemented/tested/released/unknown). Decisions must be `decided` | 023 |
| Checkpoint: `session_id`, covered turn range, `last_state`, `open_loops[]` | Adds `checkpoint_id`/`checkpoint_revision`, `branch_id`, `covered_events {from_sequence,to_sequence}`, `coverage_hash` and `last_completed_turn_id`. The referenced artifact must be `reviewed` | 024 |
| "Explicit remember/inspector saves may commit validated records" | Commit only through an owner review on a trusted surface, backed by a `manual_assertion` source. Agents can only propose | 022 |
| "Stage, validate, flush, atomically replace, then update SQLite" | A commit is immutable revisions + a complete CommitManifest catalog + one `CURRENT`. The index follows asynchronously | 021 |
| Capsule fields from v0.2 | All kept. Adds `schema_version`, `request_id`, `as_of`, `vault_commit_id`, epochs, compiler/ranking/tokenizer versions, client surface, destination, purpose, session/branch, currency per item, `verification_needed` and `completeness`. `budget.max_tokens` is kept inside `budget` | 025 |
| "Inspector shows the actual capsule sent" | Separate Inspection and Dispatch records. Dispatch memory references ⊆ capsule items | 025 |
| Future MCP tool `memory_update` | Kept as an alias of `memory_propose` (revise/supersede only). It always returns a pending candidate | 022 |
| Shared `ErrorCode` (Activity-oriented) | Unchanged. The Memory IPC uses a separate `MemoryErrorCode` set and reuses `ComponentId` from `common-v1` | 020 |
| Priority P0–P4 (source material) | P0–P2 priority. P3 maps to `sensitivity`, P4 to `volatility` | 023 |

## 2. Key contract decisions made in MV-0

- **IDs**: `<prefix>_<lowercase UUID v4>`, with 30 prefixes (`ids::ID_PREFIXES`, mirrored in `contracts/memory/common-v1`). IDs never encode time, titles, paths or accounts. Upstream IDs stay opaque strings on the SourceRecord.
- **Time**: stored timestamps are exactly `YYYY-MM-DDTHH:MM:SS.mmmZ`, with a real calendar date, so their text order equals their time order. Raw upstream strings, time zones and precision stay on the source. `BusinessTime` is a timestamp or `"unknown"`.
- **Versions**: `schema_version` (the record's major version, currently 1), `revision` (per logical record) and `commit_id`/`sequence` (the Vault) are three separate things. Before any typed parsing, an unknown major is classified `UnknownMajorReadOnly`: the bytes are kept and never rewritten. A missing or non-integer version is `Malformed`.
- **Unknown fields**: strict. Every record type rejects unknown top-level keys. Future non-security fields go in `extensions` under a dotted namespace. They are preserved verbatim, never read by policy logic, and `enouia.*` is reserved and currently rejected. Unknown enum values, including sensitivity, status and permission-like values, are errors, never mapped to a default.
- **Canonical bytes**: UTF-8, keys sorted, 2-space indent, LF, one trailing LF. Digests go in the commit catalog, never inside the record itself.
- **Flat memory records**: type-specific fields sit at the top level, as M0 describes. Serde rejects unknown keys through the type-specific struct that receives the flattened remainder.
- **Deletion**: an owner-only `delete` proposal kind plus a `confirm_delete` review producing a Tombstone. Agents cannot request it; IPC propose rejects it. This fills a gap: the design requires a review ID on tombstones but listed no delete review action.
- **Checkpoint review**: a canonical checkpoint memory references the reviewed artifact revision explicitly.
- **Temporal rule for unknown effective time**: the replacement applies from its own `valid_from`, or else from the owner's approval time. Earlier `as_of` queries keep the old fact and flag `supersession_time_unknown`.
- **Egress table**: `policy::egress_rule`, following ADR-025.
- **Mock budget**: `utf8_bytes_v1`, a deterministic counter that is not an upper bound for real tokenizers.

## 3. Ports and error semantics (MV-0.3)

Reused from `enouia-common`: `Clock`/`FakeClock`, `Cancellation`, `LockProvider` (the OS-level single-writer lock; an in-process mutex does not count) and `AtomicFile`. New in `enouia_memory_contract::ports`, as traits only:

| Port | Contract |
|---|---|
| `IdSource` | 16 random bytes → typed v4 ID. `SequentialIdSource` for tests. The production CSPRNG arrives in MV-1 |
| `VaultReader` | `pin_current` returns a `CommitPin` (commit, sequence, epochs). If `CURRENT` is unverifiable it returns `vault_recovering`, never a guess |
| `VaultWriter` | `commit(CommitRequest)` holds the OS writer lock for the whole call, checks `expected_commit_id` and per-record expected revisions (`revision_conflict`), and publishes one `CURRENT`. It returns `Committed`, or `Replayed` with the stored receipt. `find_receipt` looks a receipt up by idempotency scope |
| `classify_retry` (pure) | Same scope + same payload hash → Replay. Same scope + different payload → `idempotency_conflict`. Never an overwrite |
| `commit_time` (pure) | Clock earlier than the parent commit → `clock_regression`. The writer pauses rather than forging an order |
| `AuditSink` | A failing audit write fails external reads and dispatch closed (`audit_unavailable`) |
| `PolicyGate` | Current policy/deletion epochs. Unknown principals and destinations are denied with a non-disclosing code |
| `ProviderPort` | `capabilities()` plus `send(ProviderRequest)`. The request is built only from a DispatchRecord (`matches_dispatch`). The response is never a memory write |
| `SecretStore` / `SecretBytes` | Not Serialize or Clone, redacted Debug. Secrets cannot reach the Vault, logs, IPC or backups |
| `BackupPort` | Backs up a pinned commit under a lease. `RestorePlan` keeps the network disabled until tombstones and revocations are reconciled |

Error codes: `MemoryErrorCode::ALL` (23 codes). Retry defaults come from `default_retryable`. A conflict is never fixed by retrying with a different payload. `unauthenticated`, `permission_denied` and `not_found` are non-disclosing. IPC errors carry `{code, component, retryable}` only: no text, paths, stack traces or tokens. `MemoryError::from_common` maps shared port errors, and Activity-only codes become non-retryable `storage_failed` instead of a guessed meaning.

## 4. Dependency direction

```text
enouia-common  ←  enouia-memory-contract  ←  (future) enouia-memory / -context / -session / -provider / -core
      ↑
enouia-activity-contract ← enouia-activity, enouia-windows-process, enouia-activity-store, enouia-activity-delivery
```

`enouia-memory-contract` depends only on `enouia-common`, `serde` and `serde_json`. It has no filesystem, network, process, environment or clock access, and no `include_str!`. No Activity crate depends on it, and it depends on no Activity crate. `tests/boundaries.rs` enforces all of this, and `cargo tree` confirms it. State needed for recovery (reviews, idempotency receipts, tombstones, checkpoints, session events) is defined as file records under `vault/`. None of it exists only as a SQLite design. `layout::MANAGED_ROOTS` never includes `activity/`.
