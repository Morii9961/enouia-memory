# Architecture decision register

Status words: **Adopted** means the contract takes this direction now. **Adopted (contract)** means the contract is frozen but the store behavior that proves it is later-stage work. **Direction recorded** and **Deferred** activate nothing. Behavior that needs a real store, an installed product, or an external resource is claimed only after its stage passes.

## ADR-MEM-01 … 18 — design package decisions

These come from [design/DECISIONS_AND_SOURCES](../design/DECISIONS_AND_SOURCES.md) §1 and keep their IDs:

| ID | Decision | Status |
|---|---|---|
| ADR-MEM-01 | Local Primary; single owner, several principals | Adopted |
| ADR-MEM-02 | Raw / Canonical / Candidates / Session / Index layers | Adopted |
| ADR-MEM-03 | Canonical JSON, Identity Markdown, rebuildable index | Adopted |
| ADR-MEM-04 | Immutable revisions + commit catalog + single `CURRENT` | Adopted (contract); behavior in MV-1 |
| ADR-MEM-05 | Trusted owner approval | Adopted; sharpened by ADR-MEM-22 and ADR-MEM-30 |
| ADR-MEM-06 | priority / sensitivity / volatility are independent | Adopted |
| ADR-MEM-07 | Valid time + known time + evidence status | Adopted; amended by ADR-MEM-32 |
| ADR-MEM-08 | Automatic checkpoint is a Session artifact | Adopted |
| ADR-MEM-09 | Local FTS/metadata; separate CJK short-query path | Direction recorded (frozen in MV-4) |
| ADR-MEM-10 | Inspectable Dispatch; egress authorized separately | Adopted; made concrete by ADR-MEM-25, 30 and 33 |
| ADR-MEM-11 | Embedded Core → single Memory Host | Direction recorded (MV-8) |
| ADR-MEM-12 | Activity is independent | Adopted (Activity stays in Runtime) |
| ADR-MEM-13 | ACL / verified volume protection + restic backup | Direction recorded; gated before real data |
| ADR-MEM-14 | Raw immutable by default; owner deletion is the exception | Adopted (contract); purge in MV-3 |
| ADR-MEM-15 | Gateway at-least-once + local idempotency | Deferred (MV-9) |
| ADR-MEM-16 | Encrypted device replica ≠ server-computable subset | Deferred (MV-10) |
| ADR-MEM-17 | Single primary; offline edits become candidates | Deferred (MV-10) |
| ADR-MEM-18 | Adapters open only after measurement | Adopted |

## ADR-MEM-19 … 38 — implementation decisions (MV-0 / MV-0R / MV-1 / MV-2)

ADR-MEM-20 to 29 were first drafted with Enouia Runtime's register numbers 020–029 and never committed there. The draft is kept in [history](../history/adr-draft-runtime-numbering.md). Numbering here is this repository's own.

### ADR-MEM-19 — Independent repository (Adopted, MV-0R)

Enouia Memory is developed in its own repository (`Morii9961/enouia-memory`) with its own version history. The earlier handoff placed it inside the Enouia Runtime workspace; that arrangement is withdrawn. This repository builds from its own checkout, with no path, Git, or build dependency on Runtime. Memory owns the minimal host ports it needs (`foundation`: Clock, Cancellation, WriterLock, AtomicFile, ComponentId). Runtime integrates through versioned JSON/IPC contracts, or through a pinned release or Git revision of this repository, plus its own adapters. The default runtime data root is `%LOCALAPPDATA%\EnouiaMemory`, separate from Runtime's own data root, and it can be replaced by an explicit, verified local path.

### ADR-MEM-20 — Contract crate and Memory error codes (Adopted)

`enouia-memory-contract` holds the pure types, strict parsing, validators, and ports. Its only dependencies are Serde and serde_json. Memory IPC has its own `MemoryErrorCode` set: the INTERFACES §1 codes plus `vault_recovering`, `invalid_request`, `unsupported_budget`, `clock_regression`, and `intentionally_purged`. It also has its own `componentId`. A host maps both onto its health DTOs.

### ADR-MEM-21 — Commit catalog details (Adopted as contract)

This refines ADR-MEM-04. The manifest carries the parent commit, sequence, full logical-record catalog (including single-revision reviews, approvals, tombstones, receipts, and session events), content digests, epochs, the idempotency-key digest, the payload digest, and the receipt. When `CURRENT` cannot be verified the Vault reports `vault_recovering` and becomes read-only. Atomicity and durability are MV-1 evidence.

### ADR-MEM-22 — Only candidates from models and agents (Adopted)

Models, agents, extraction, and imported text create candidates only. A canonical write needs an owner `ReviewRecord` on a trusted surface, bound to one pending candidate revision and one nonce. Deletion is owner-only: a `delete` proposal plus a `confirm_delete` review. IPC propose cannot request it.

### ADR-MEM-23 — Axes and valid/known time (Adopted; amended by 32)

priority P0–P2, sensitivity, and volatility are separate fields. Valid time is the half-open interval `[valid_from, valid_until)`. Known time is the pinned commit catalog. A replacement that takes effect in the future never hides a fact that is still current. Project decisions, plans, implementation, tests, and releases are separate `state_kind` items.

### ADR-MEM-24 — Checkpoint artifact vs reviewed memory (Adopted)

An automatic checkpoint is `provisional`. A canonical `session_checkpoint` memory must reference a `reviewed` artifact revision.

### ADR-MEM-25 — Capsule / Inspection / Dispatch (Adopted; extended by 30 and 33)

Each context request produces a Capsule (which now records `requested_by`), an Inspection that is never sent, and a Dispatch when content is actually sent. A Dispatch lists every resource it carries (`resource_refs`: memory, Identity, checkpoint, session event, source, attachment). Each of those resources must be in the Capsule.

### ADR-MEM-26 — Deletion lifecycle (Adopted as contract; purge MV-3; amended by 30)

Supersede, logical delete, and purge are distinct operations. A tombstone overrides every status. A purge receipt never claims global erasure.

### ADR-MEM-27 / 28 / 29 — Host, backup, remote route

These keep the directions of ADR-MEM-11, 13, and 15–17. Nothing is activated by them.

### ADR-MEM-30 — Approvals are bound to what they approve (Adopted, MV-0R, fixes F1 and F2)

Consent to send content outside, consent to lower a sensitivity level, and a policy grant are separate owner decisions. None of them may be inferred from a memory review. `ApprovalRecord` (`apv_…`) is issued by the owner on a trusted surface with a single-use nonce. Its explicit `binding` is one of:

- **egress**: request, capsule, payload digest, full destination (kind + provider + model + adapter), the exact resource revisions, policy, and policy epoch. It expires within 15 minutes and is used by at most one Dispatch. It is required for private content sent to an external destination.
- **declassification**: exact target revision, `from` (strictest source) and `to` levels, the source set, and the content digest.
- **policy_grant**: policy ID, revision, and the digest of that revision's canonical bytes.

`confirm_delete` reviews carry a `delete_binding` (mode, scope, targets) that must equal the tombstone. Record fields that reference approvals are typed `ApprovalId`, so a review ID cannot even be written there.

### ADR-MEM-31 — Persistent policies and default-deny evaluation (Adopted, MV-0R, fixes F4)

`PolicyRecord` (`pol_…`, versioned, with status and revocation) selects by principal, scope, project (or all projects), record kind, maximum sensitivity, purpose, and destination (kind, optionally exact provider and model). The Vault begins with a `genesis_default` policy that is owner-only, uses local destinations, and never grants `provider:send`. Every other policy is an `owner_grant` bound to a `policy_grant` approval. `policy::evaluate` is default-deny: it uses the latest revision of each policy, and among the allowing policies it picks the lowest policy ID, so the decision is deterministic. The frozen egress table still applies on top, so highly sensitive content is never sent automatically. `PolicyGate` receives server-resolved targets (record revision, project, sensitivity) and the full destination. Capsule inclusion checks `context:read` access, and for external destinations also `provider:send` through the record's egress policy. Dispatch rechecks against the latest policies at send time, so a revocation takes effect.

### ADR-MEM-32 — Unknown supersession start stays unknown (Adopted, MV-0R, fixes F5)

Only an explicit, owner-confirmed `effective_from` establishes when a replacement starts. Unknown never falls back to `valid_from` or to the approval time. Both the old and the new fact stay in effect, marked `needs_reverification`, for any `as_of`. Each edge's `effective_from` must equal the value recorded in its supersede review. This supersedes the MV-0 draft rule.

### ADR-MEM-33 — Request payload digest (Adopted, MV-0R, fixes F3)

`request_hash` = SHA-256 over the canonical bytes of `{payload_version: 1, destination, messages[{role, content_hash, size_bytes}], tools[{name, definition_hash}], output{max_output_tokens, streaming}}`. Content and definition hashes are SHA-256 over the exact UTF-8 bytes. Dispatch records must equal their recomputed digest. `ProviderRequest::verify_against` is the send gate, and it recomputes everything from the actual request. Full message bodies live in private Session objects addressed by `content_hash`; storing them, checking their integrity, and propagating their deletion are MV-1/MV-5 work.

### ADR-MEM-34 — Numeric range parity and checked arithmetic (Adopted, MV-0R, fixes F6)

Every JSON integer on the wire or in storage lies within ±(2^53 − 1), as the schemas state. The Rust parsers enforce this before typed parsing and return `number.out_of_range`. Cross-field sums use checked arithmetic, and no validator iterates a range supplied by the caller.

### ADR-MEM-35 — Independent schema cross-check (Adopted, MV-0R)

Every schema has a unique absolute `$id` under the reserved host `contracts.enouia-memory.invalid`. Besides the in-repo subset validator used by `cargo test`, a pinned python-jsonschema 4.26.0 (Draft 2020-12) checks every schema against the metaschema and re-verifies every fixture expectation (`tools/schema-check`).

### ADR-MEM-36 — Vault store file contracts (Adopted, MV-1)

The store's own files are versioned contracts under `contracts/store/` with Rust types in `store.rs`: `vault/vault.json` (descriptor, written once by genesis), `vault/CURRENT` (names one commit and pins the SHA-256 of its manifest bytes), `vault/journal/published.jsonl` (one line appended after each publication, used as recovery evidence), `vault/idempotency/<scope>.json` (trusted only when the named commit is on the published chain), `vault/recovery/<id>.json` (the owner's explicit recovery choice; evidence is `publish_journal`, `verified_unpublished`, or `restored_export`, never "newest manifest"), `export-manifest.json` (a pinned-commit export: safe `vault/` paths only, sorted and unique, bookkeeping excluded), and `config/restore-state.json` (network stays disabled until reconciliation).

Port refinements: a `CommitRequest` always carries an idempotency scope, and scopes are keyed by commit `OperationKind` (imports have no IPC operation). A `StagedObject` carries its `ObjectKind`. A manifest's `objects` list, like its catalog, is complete: every object reachable at that commit. Content-addressed objects live under `raw/objects`, `assets/objects`, and `session-content/objects`; Identity Markdown is stored beside its revision.

The caller pre-assigns `commit_id` (from its `IdSource`), because a review names the commit that applies it; the store refuses an existing ID. A commit that catalogs an Identity revision must carry its Markdown object, so the synthetic lifecycle fixture now lists that object (`sets/lifecycle-objects.json` holds its bytes). Catalog entries and receipt records are ordered by record-kind name, then ID. Every commit is checked against every cross-record rule over the complete history before it is published (`VaultOptions::validate_record_set`, on by default).

### ADR-MEM-37 — Vault store implementation (Adopted, MV-1)

`enouia-memory-vault` implements the store on Windows/NTFS with these choices:

- **Data root.** Enabled only after `verify_data_root`: absolute, canonical (no 8.3 or `subst` indirection), a local fixed disk, NTFS or ReFS, no reparse point on the path, no cloud-file attributes, not under a sync-client folder (OneDrive environment roots plus known client folder names), a Git working tree, Program Files, or Windows, and a free-space floor. Nothing creates `%LOCALAPPDATA%\EnouiaMemory`; every entry point takes an explicit root.
- **Managed paths.** Relative names of `[A-Za-z0-9_.-]` under the managed roots only (no `..`, device names, colons). Every existing component between the root and a target is checked for reparse points before each operation. A check-then-use window remains against a same-user attacker; it is documented, not closed.
- **Writer lock.** An exclusive `LockFileEx` on `vault/LOCK` through `File::try_lock`, held by an open handle and released by the OS when the process ends. Waiting is bounded; the result is `busy`.
- **Durability primitives.** Files are flushed (`FlushFileBuffers`) before use; replacements go through `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` in the same directory. `CURRENT` is the only publication step. The claim is limited to process crashes (V09).
- **Transaction.** Staging, then final names, then the manifest, then the idempotency entry, then `CURRENT`, then the journal. Anything written before `CURRENT` is unreferenced; a file found at a name the transaction must write is moved to `vault/orphans/`, never deleted. A manifest left under the caller's commit ID by a crashed attempt is quarantined the same way; only a manifest on the published chain counts as a reused ID.
- **Validation on commit.** Every staged record is strictly parsed, must be in canonical bytes, and must be the next revision; then `set::validate_set` runs over the complete history plus the new commit before anything is written.
- **Leftovers.** An owner-invoked sweep, under the writer lock and only while `CURRENT` verifies, moves every file the published chain does not reference (revisions, objects, manifests, idempotency entries, replace temporaries) to `vault/orphans/<sweep_id>/`. Nothing is deleted.
- **Recovery.** A damaged `CURRENT` makes reads and writes return `vault_recovering`. `recovery_report` lists publish-journal evidence first, then complete but never-published manifests; nothing is chosen automatically. `adopt_recovery_point` needs the owner and a complete candidate and writes a receipt before replacing `CURRENT`.
- **Backup exit.** A pinned-commit export (descriptor, whole chain, every cataloged revision, head objects, sealed audit segments) is verified independently and restored only into an empty verified root, with network use gated until reconciliation. Encryption is restic's job; the adapter passes the password only through a cleared child environment.
- **Audit and health.** Audit lines are validated `AuditEvent`s in size-rotated JSONL segments; health reads never write.
- **Measured cost and trigger.** A complete catalog costs about 241 bytes per record in every manifest, so cumulative manifest bytes grow quadratically with single-record commits, and full-history validation grows with the Vault. MV-1 keeps this for simplicity and verifiability. The segmented or incremental catalog (and incremental validation) is required before a head manifest exceeds 1 MiB (about 4,000 records) or commit latency exceeds 1 s on the reference machine, whichever comes first; real history import (MV-2) must be measured against this trigger.

### ADR-MEM-38 — ImportManifest as a revisioned record (Adopted, MV-2)

Each user-selected import is a revisioned `import` record (`imp_…`) stored at `vault/raw/manifests/<id>/<revision>.json`, as the layout specified. A new revision is committed at each step (archived, every parse batch, completion) in the same transaction as the sources that step produced, so the resume cursor, adapter version, counts, and coverage always describe exactly what that commit contains. It records the input kind, the received bytes' hash and size, the adapter and observed source schema, sanitized archive members with their disposition (parsed, preserved only, quarantined, skipped), the cursor, counts, per-conversation coverage (upstream IDs, counts, times; never titles), warnings, and `duplicate_of` for a byte-identical re-import.

Closure rules: an imported source names an existing import and cites that import's received bytes (`source.import_unresolved`, `source.import_raw_mismatch`); a completed import's coverage counts exactly the sources citing it (`import.coverage_mismatch`); an import past `archiving` is committed only with its Raw object present (`store.import_raw_missing`). Raw completeness and parse completeness are separate: an unrecognized format is archived and ends `partial` with no adapter. The synthetic fixtures now carry an import manifest and its raw bytes for every imported source.

## Relation to Enouia Runtime's register

Runtime ADR-001 (local canonical ownership), 002/003 (human-readable Memory, disposable index), 006 (Memory schema and candidate lifecycle), 007 (Mock Provider first), 008/009 (deferred replica and bridge), 010 (encryption decision deferred), 011 (embedded Core), and 012 (independent Activity) remain Runtime's view of the integration boundary. ADR-MEM-03, 05, 07, 10, 11, 13, 15–17, and 22 refine them from the Memory side. A Runtime-side ADR recording the dependency on this repository belongs to Runtime's next integration change; it is not part of this repository.
