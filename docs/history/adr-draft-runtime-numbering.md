# ADR-020 – ADR-029 — Memory extension v1 (MV-0)

> Links that pointed into the Runtime working-tree layout were turned into plain text when this file moved to `docs/history/`.

> **Draft, pending MV-0R renumbering and correction.** These ADRs were drafted under Runtime's ADR register numbers 020–029 and never committed there. MV-0R moves them into this repository's own register.

Recorded 2026-09-28 during MV-0. These decisions register the Memory design package v1.0 against Architecture v0.3 and ADR-001–019. "Adopted" means the contract now takes this direction. Behavior that needs a real store, installed product, or external resource stays unimplemented until its stage passes. "Direction recorded" / "Deferred" activates nothing. Activity Track B (B1–B5) is not reordered or extended by any of these.

## ADR-MEM → repository mapping

The design package numbers its decisions ADR-MEM-01…18 locally. They map as follows:

| Design ID | Repository ADR | Relation | Status here |
|---|---|---|---|
| ADR-MEM-01 Local Primary, single owner / many principals | ADR-001 + ADR-020 | Reaffirms ADR-001. ADR-020 adds principal kinds and scopes | Adopted |
| ADR-MEM-02 Raw / Canonical / Candidates / Session / Index layers | ADR-002, ADR-003 + ADR-020 | Reaffirmed. Review, deletion and idempotency must live in files | Adopted |
| ADR-MEM-03 Canonical JSON, Identity Markdown, rebuildable index | ADR-002, ADR-003 | Unchanged. Identity gains a sidecar metadata revision | Adopted |
| ADR-MEM-04 Immutable revisions + commit catalog + single CURRENT | **ADR-021** | Refines Architecture v0.3 §6 "stage, validate, flush, atomically replace" | Adopted (contract), MV-1 behavior |
| ADR-MEM-05 Trusted owner approval | **ADR-022** | Tightens ADR-006's "explicit saves may commit" | Adopted |
| ADR-MEM-06 priority / sensitivity / volatility independent | **ADR-023** | New | Adopted |
| ADR-MEM-07 Valid time + known time + evidence status | **ADR-023** | New | Adopted |
| ADR-MEM-08 Automatic checkpoint is a Session artifact | **ADR-024** | Refines ADR-006 (fifth type) | Adopted |
| ADR-MEM-09 Local FTS/metadata, CJK short-query path | ADR-003, ADR-007 | Recorded only. Ranking and tokenizer are frozen in MV-4 | Direction recorded (MV-4) |
| ADR-MEM-10 Inspectable Dispatch, separate egress authorization | **ADR-025** | Extends ADR-007 and the v0.3 §6 capsule | Adopted |
| ADR-MEM-11 Embedded Core → single Memory Host | **ADR-027** | Refines ADR-011 for Core only | Direction recorded; activation at MV-8 |
| ADR-MEM-12 Activity independent | ADR-012 | Inherited unchanged | Adopted (existing) |
| ADR-MEM-13 ACL + verified volume protection + restic backup | **ADR-028** | Narrows ADR-010. App-level encryption stays deferred | Direction recorded; gated before real data |
| ADR-MEM-14 Raw immutable by default, owner deletion | **ADR-026** | New | Adopted (contract), MV-3 purge |
| ADR-MEM-15 Gateway at-least-once + local idempotency | **ADR-029** | Refines ADR-009 | Deferred (MV-9) |
| ADR-MEM-16 Encrypted device replica ≠ server-computable subset | **ADR-029** | Refines ADR-008 | Deferred (MV-10) |
| ADR-MEM-17 Single primary; offline edits become candidates | **ADR-029** | Refines ADR-008 | Deferred (MV-10) |
| ADR-MEM-18 Adapters opened only after measurement | ADR-020 | Principle | Adopted |

Mapping from the old milestones: MV-0 finishes the "A1 schema" handoff from the M0 agreement. MV-1, MV-3 and MV-4 refine the old A1, MV-5 corresponds to A2, and MV-6 to A3. MV-2, history import, moves ahead of the v0.2 "product 0.2" import. Track B milestones and their gates are unchanged.

## ADR-020 — Memory design package v1.0 and MV milestones (Adopted)

Context: Architecture v0.3 set Memory boundaries, and M0 left full schemas to A1. The design package puts history protection first and supplies full semantics.

Decision: the package in `docs/memory/design/` is the target specification for the Memory/Context/Session/Provider domain. Where the two conflict, Architecture v0.3 and this register win until an ADR here records a change. MV-0…MV-11 replace A1–A3 as the Track A plan. New Memory contracts live in `contracts/{memory,context,provider}` and `contracts/ipc/memory-v1.schema.json`. Pure types and validators live in the new `enouia-memory-contract` crate, mirroring `enouia-activity-contract`. It depends only on `enouia-common`, Serde and serde_json. Memory-domain IPC errors use a separate `MemoryErrorCode` set: the INTERFACES §1 codes plus `vault_recovering`, `invalid_request`, `unsupported_budget`, `clock_regression` and `intentionally_purged`. The shared `ErrorCode` in `common-v1` is unchanged, so the Activity UI contract stays as it is.

Consequences: `enouia-memory`, `-context`, `-session` and `-provider` crates are created only when a stage needs them. They consume this contract crate. The Architecture v0.3 §4 dependency rule still applies in both directions: no Activity ↔ Memory dependency.

## ADR-021 — Commit catalog transaction (Adopted as contract; behavior MV-1)

Decision: a Vault write is a transaction made of immutable revision files, one complete `CommitManifest` and a single `CURRENT` naming it. The manifest carries the parent commit, sequence, full logical-record catalog, object hashes, review and tombstone references, policy and deletion epochs, the idempotency key hash, the request payload hash and the operation receipt. Replacing several files independently does not make a cross-file transaction. When `CURRENT` cannot be verified, the Vault enters `vault_recovering` and becomes read-only. It never guesses the newest commit.

Supersedes: the Architecture v0.3 §6 sentence "stage, validate, flush, atomically replace, then update SQLite" is refined. The atomic step is the single `CURRENT` publication of a complete commit, as ADR-014 already does for Activity generations.

Evidence limit: MV-0 validates manifest shape, the chain, epochs, idempotency and catalog closure over synthetic sets. Atomic visibility, crash recovery and flush durability are unproven until the MV-1 fault-injection tests pass. Power-loss durability needs separate evidence (V09).

## ADR-022 — Trusted owner approval boundary (Adopted)

Decision: models, agents, extraction and imported text can only create `CandidateRecord`s. A canonical write needs a `ReviewRecord` with `actor_type: owner` on a `trusted_windows_app` or `trusted_local_cli` surface. The review is bound to one exact candidate revision, a final content hash, a diff hash and a single-use nonce. The "remember …" quick path creates an owner candidate and its review in one commit, backed by a `manual_assertion` source. An `agent_submission` source can never carry user evidence, whatever it claims about consent. Model `confidence` is advisory only. Deletion is owner-only: the `delete` proposal kind plus a `confirm_delete` review. It is not exposed through IPC propose, so agents cannot use it.

Supersedes: ADR-006's "explicit inspector saves/remember requests may commit validated records" now reads "may commit through the owner quick path with a manual assertion and review record".

## ADR-023 — Independent axes and two time axes (Adopted)

Decision: `priority` (P0–P2), `sensitivity` (public/normal/private/highly_sensitive) and `volatility` (stable/changing/live) are separate fields. The source material's P3 (sensitive) maps to sensitivity and P4 (volatile) to volatility. Valid time is a half-open `[valid_from, valid_until)` interval. Supersession edges carry `effective_from` (a timestamp or `"unknown"`). Known time is the pinned commit. A replacement that takes effect in the future never hides a fact that is still in effect. An unknown effective time applies the replacement only from the replacement's `valid_from`, or else from the owner's approval time, and flags earlier queries as `supersession_time_unknown`. ProjectState records decisions, plans, implementation, tests and releases as separate items with `state_kind`.

## ADR-024 — Checkpoints: provisional artifact vs reviewed memory (Adopted)

Decision: an automatic `SessionCheckpoint` is a Session artifact with status `provisional`, `reviewed` or `stale`. A canonical `session_checkpoint` memory must reference a checkpoint revision whose status is `reviewed`. Capsules mark provisional checkpoints and their open loops as provisional.

## ADR-025 — Capsule, Inspection, Dispatch and egress (Adopted)

Decision: every context request yields a Capsule (the logical context), an Inspection (the local explanation, never sent) and, when something is sent, a Dispatch (roles, byte hashes, the capsule memories carried, tools and the egress barrier used). A Dispatch may only carry memories that the Capsule includes. Default egress: `highly_sensitive` is never sent automatically. Local destinations may receive public, normal and private content. External destinations need a standing egress policy, and `private` content also needs a per-request owner confirmation. Tombstones and policy epochs are rechecked before sending. The capsule keeps the Architecture v0.3 fields and adds versioning, epochs, destination, purpose, currency labels, verification needs and completeness. The Mock estimator is `utf8_bytes_v1`. It is deterministic but is not an upper bound for any real tokenizer.

## ADR-026 — Deletion lifecycle (Adopted as contract; purge MV-3)

Decision: correction (supersede), logical delete (a tombstone and deletion epoch, with the body retained under restriction) and purge (bodies removed, with a text-free receipt) are three distinct operations. A tombstone overrides every status and Raw's default immutability. A `PurgeReceipt` has no "globally erased" state. It reports pending stores, including backups and replicas, until they are confirmed.

## ADR-027 — Embedded Core, then a single Memory Host (Direction recorded; activates at MV-8)

The v0.3 embedded Core remains for MV-1 to MV-7. At MV-8 one user-level Host owns the Vault writer lock and the UI, CLI and MCP adapters connect to it. The UI never becomes a second writer. No SYSTEM service. Nothing in MV-0 depends on this.

## ADR-028 — Local protection and backup route (Direction recorded; gated)

User-restricted ACLs, verified volume protection and a restic adapter backing up pinned commits. The recovery secret is held independently of the device's DPAPI. This narrows but does not close ADR-010: application-level encryption is still undecided, and no encryption is claimed. Gate: before any real private history is imported, both the local protection check and a successful restore to a new location must pass. MV-0 defines only the `BackupPort` and `SecretStore` ports.

## ADR-029 — Remote route (Deferred)

The VPS Gateway (MV-9) is an at-least-once durable queue with local idempotent receipts. Encrypted backup (G2), an encrypted device replica (G3) and a server-computable subset (G4) are separate capabilities, each opted into on its own. There is a single primary writer, and offline edits come back as candidates. None of this is implemented or activated; ADR-008 and ADR-009 remain the governing deferred records.
