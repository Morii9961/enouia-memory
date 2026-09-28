# MV-0 constraint enforcement map

> **Draft (pre-MV-0R).** Written against the Runtime draft; corrected in MV-0R.

For each constraint family, this map shows who enforces it:

- **Schema**: the JSON Schemas under `contracts/`, checked by the in-repo subset validator in `crates/enouia-memory-contract/tests/support/schema.rs`. That validator is not an independent implementation of JSON Schema. It panics on any keyword it does not implement, and it treats `format` only as an annotation.
- **Rust record**: `parse_value` and each record's `validate()`.
- **Rust set**: `set::validate_set` over a whole record set.
- **Later behavior**: tests that need a real store, installed product, or resource, and so cannot pass in MV-0.

**Passing Schema and Rust validation does not prove that a transaction is atomic, durable, or recoverable.**

| Constraint family | Schema | Rust record | Rust set | Later behavior |
|---|---|---|---|---|
| Required fields, explicit `null` for unknown | ✔ `required` | ✔ serde `nullable` (a missing key is an error) | – | – |
| Unknown fields rejected, namespaced extensions preserved | ✔ `additionalProperties:false`, `propertyNames` | ✔ `deny_unknown_fields`, flattened variant structs, reserved `enouia.*` | – | MV-1 migration keeps extensions |
| Enums: no unknown sensitivity, status or permission | ✔ | ✔ | – | – |
| ID namespaces (`<prefix>_<uuid v4>`) | ✔ pattern | ✔ typed IDs; `ref.kind_prefix` for generic refs | – | MV-1 `IdSource` CSPRNG |
| UTC millisecond timestamps | ✔ pattern | ✔ plus real calendar date (Feb 30 is rejected only here) | – | – |
| Time order (created ≤ updated, approval ≤ update, valid interval half-open, occurred ≤ captured, episode interval) | ✗ | ✔ | commit `created_at` non-decreasing | MV-1 `commit_time` with a real Clock |
| Major version: unknown major is read-only | ✔ `const 1` | ✔ `schema_disposition`, `ensure_writable` | – | **D02 behavior (MV-1)**: a failed migration keeps the old CURRENT; restore from the pre-upgrade snapshot |
| Source role vs evidence class (no assistant/agent user evidence; manual assertion only from the owner) | ✔ `if/then` (partial) | ✔ `source.role_confusion`, `source.manual_operator` | ✔ `evidence.class_mismatch` (citation cannot upgrade the class) | – |
| Import provenance (import ID and raw hash; account alias is not a login) | ✔ | ✔ | – | MV-2 hashes over received bytes |
| Evidence closure (source revision exists, same anchor hash, locator within source) | ✗ | primary source ∈ evidence; item evidence ⊆ record evidence | ✔ `evidence.*` | MV-1 recomputes object hashes |
| Missing source → `broken_provenance`, never auto-sent | enum | – | ✔ unresolved evidence is allowed only when `provenance_state = broken`; the capsule rejects broken records | – |
| Sensitivity never downgraded without a declassification review | ✗ | – | ✔ `sensitivity.downgrade` | – |
| Secrets not in memory, candidate, capsule or dispatch text (heuristic) | ✗ | ✔ `*.secret_material`, `dispatch.credential_material` | – | MV-2 quarantine; B04 log scan |
| No local absolute paths in capsule or dispatch | ✗ | ✔ `*.local_path` | – | – |
| Candidate lifecycle: pending → terminal only; terminal is final; merge target; reopen by a new candidate | ✔ per-revision `if/then` | ✔ field consistency | ✔ `candidate.status_transition`, `candidate.reopen_target` | MV-3 review engine |
| Canonical write requires an owner review of the exact pending revision | ✔ `ownerRef`, trusted-surface enum | ✔ `review.owner_required`, nonce format | ✔ `memory.review_missing`, `memory.unreviewed_candidate`, `review.stale_candidate_revision`, `memory.approval_mismatch`, `review.nonce_reuse`, `review.commit_ref` | **MV-3**: nonce issuance, expiry, diff-hash recomputation, batch all-or-nothing (M02/M03) |
| Agents cannot request deletion or review | ✔ | ✔ `candidate.delete_owner_only` | – | MV-8 transport authentication (X02) |
| Supersession: precise target, no self-edge, no cycle, same scope, status consistent | ✗ | self/duplicate edge | ✔ `supersession.*` | – |
| Future-effective replacement does not hide a current fact; valid time vs known time | ✗ | – | ✔ `temporal::effect` / `currency`; capsule uses the commit catalog as `known_at` | MV-4 `as_of`/`known_at` queries (M06) |
| Conflicts shown on both sides | ✗ | capsule conflict flag | ✔ `conflict.*`, `capsule.conflict_one_sided` | MV-5 compiler (M07, C08) |
| ProjectState: decided ≠ implemented ≠ released | ✔ decisions `const decided` | ✔ `memory.decision_kind` | – | MV-5 (C02) |
| Checkpoint: provisional artifact vs reviewed memory; coverage closure | ✔ status/review `if/then` | ✔ | ✔ `checkpoint.*`, `memory.checkpoint_unreviewed` | MV-5 session recovery (S03/S04) |
| Session events: sequence unique, parent earlier, completion needs input, no hidden reasoning | ✔ kind enum, delivery `if/then` | ✔ | ✔ `event.*` | MV-5 streaming persistence (S01/S02) |
| Commit manifest: genesis shape, complete catalog, receipt equals changed records | ✔ genesis `if/then` | ✔ `commit.*` (record) | ✔ chain, epochs, catalog closure/mismatch | **MV-1**: atomic `CURRENT` publication, flush, crash boundaries (V02), power loss (V09) |
| Idempotency: same key + same payload replays; different payload conflicts | ✗ | `ports::classify_retry` | ✔ `commit.idempotency_conflict`, `commit.duplicate_application` | **MV-1**: lost response → same receipt (V03) |
| Single writer | – | `LockProvider` port only | – | **MV-1**: two OS processes (V01) |
| Deletion: tombstone overrides status; purge receipts are honest | ✔ no `globally_erased` | ✔ `purge.overall_state` | ✔ `tombstone.*`, `purge.tombstone`, capsule/dispatch barriers | **MV-3**: purge propagation, backups (P01, B02) |
| Egress: highly sensitive never automatic; external needs a policy; private needs per-request confirmation | ✗ | `policy::egress_rule` | ✔ `capsule.policy_violation`, `dispatch.private_unconfirmed` | MV-7 real Provider (P03) |
| Capsule ↔ Inspection ↔ Dispatch agreement; no hidden extra memory | ✗ | reason vs decision; restricted viewers see nothing hidden | ✔ `inspection.*`, `dispatch.hidden_memory` | MV-5/MV-6 actual render (C06) |
| Deletion/policy barrier rechecked before send | ✗ | – | ✔ `dispatch.tombstoned_content`, `dispatch.stale_barrier` | MV-5/MV-7 live recheck (V08, P06) |
| Budget honesty | ✗ | ✔ `capsule.budget`, `capsule.completeness` | – | MV-5/MV-7 real counting (C04) |
| Provider capabilities not guessed | ✔ `if/then` | ✔ `capabilities.unverified_claim` | – | MV-7 smoke tests |
| IPC: request/response pairing, write idempotency key, limits (search ≤100, excerpt ≤8 KiB), propose never commits, alias `memory_update` returns pending | ✔ (per-operation `oneOf`) | ✔ `ipc.*` | – | MV-6/MV-8 transports |
| Dependency direction; no Activity/Moriium/fs/net/process in contracts; Vault paths avoid `activity/` | – | `tests/boundaries.rs` | – | **D03/D04 behavior (MV-1)**: installed artifact runs without either checkout; Activity hashes unchanged across Memory import/delete/rebuild/restore |

## Rule coverage by negative fixtures

The validators define 219 stable rule identifiers. For 126 of them, a dedicated invalid fixture or test asserts the rule by name. The other 93 are implemented, and several fire as side effects of other cases, but no fixture asserts them individually yet. Adding those cases is cheap and is listed as MV-1 hardening. They are mostly field-consistency checks on the same families as above: for example `candidate.primary_source`, `commit.receipt_operation`, `event.parent_order`, `inspection.header_mismatch`, `review.target_missing` and `session.last_event_seq`.
