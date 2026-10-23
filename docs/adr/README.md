# Architecture decision register

Status words: **Adopted** means the contract takes this direction now. **Adopted (contract)** means the contract is frozen but the store behavior that proves it is later-stage work. **Direction recorded** and **Deferred** activate nothing. Behavior that needs a real store, an installed product, or an external resource is claimed only after its stage passes.

## ADR-MEM-01 … 18 — design package decisions

These come from [design/DECISIONS_AND_SOURCES](../design/DECISIONS_AND_SOURCES.md) §1 and keep their IDs:

| ID | Decision | Status |
|---|---|---|
| ADR-MEM-01 | Local Primary; single owner, several principals | Local-primary target superseded by ADR-MEM-49; current local implementation retained |
| ADR-MEM-02 | Raw / Canonical / Candidates / Session / Index layers | Adopted |
| ADR-MEM-03 | Canonical JSON, Identity Markdown, rebuildable index | Adopted |
| ADR-MEM-04 | Immutable revisions + commit catalog + single `CURRENT` | Adopted (contract); behavior in MV-1 |
| ADR-MEM-05 | Trusted owner approval | Adopted; sharpened by ADR-MEM-22 and ADR-MEM-30 |
| ADR-MEM-06 | priority / sensitivity / volatility are independent | Adopted |
| ADR-MEM-07 | Valid time + known time + evidence status | Adopted; amended by ADR-MEM-32 |
| ADR-MEM-08 | Automatic checkpoint is a Session artifact | Adopted |
| ADR-MEM-09 | Local FTS/metadata; separate CJK short-query path | Direction recorded (frozen in MV-4) |
| ADR-MEM-10 | Inspectable Dispatch; egress authorized separately | Adopted; made concrete by ADR-MEM-25, 30 and 33 |
| ADR-MEM-11 | Embedded Core → single Memory Host | Direction recorded; authority contract frozen by ADR-MEM-66 (MV-8.1a), process switch pending |
| ADR-MEM-12 | Activity is independent | Adopted (Activity stays in Runtime) |
| ADR-MEM-13 | ACL / verified volume protection + restic backup | Direction recorded; gated before real data |
| ADR-MEM-14 | Raw immutable by default; owner deletion is the exception | Adopted (contract); purge in MV-3 |
| ADR-MEM-15 | Gateway at-least-once + local idempotency | Gateway-only target superseded by ADR-MEM-49; event reliability retained |
| ADR-MEM-16 | Encrypted device replica ≠ server-computable subset | Deferred (MV-10) |
| ADR-MEM-17 | Single primary; offline edits become candidates | Deferred (MV-10) |
| ADR-MEM-18 | Adapters open only after measurement | Adopted |

## ADR-MEM-19 … 46 — implementation decisions (MV-0 / MV-0R / MV-1 … MV-6, Runtime integration)

ADR-MEM-20 to 29 were first drafted with Enouia Runtime's register numbers 020–029 and never committed there. The draft is kept in [history](../history/adr-draft-runtime-numbering.md). Numbering here is this repository's own.

### ADR-MEM-19 — Independent repository (Adopted, MV-0R; refined by ADR-MEM-45)

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
- **Transaction.** An intent marker in `staging/`, then each record and object written once and flushed at its final (still unreferenced) name, then the manifest, then the idempotency entry, then `CURRENT`, then the journal. (MV-1 wrote to staging and renamed; MV-2 removed the rename, which halved the flushes without changing what a crash can expose.) Anything written before `CURRENT` is unreferenced; a file found at a name the transaction must write is moved to `vault/orphans/`, never deleted. A manifest left under the caller's commit ID by a crashed attempt is quarantined the same way; only a manifest on the published chain counts as a reused ID.
- **Validation on commit.** Every staged record is strictly parsed, must be in canonical bytes, and must be the next revision; then `set::validate_set` runs over the complete history plus the new commit before anything is written.
- **Leftovers.** An owner-invoked sweep, under the writer lock and only while `CURRENT` verifies, moves every file the published chain does not reference (revisions, objects, manifests, idempotency entries, replace temporaries) to `vault/orphans/<sweep_id>/`. Nothing is deleted.
- **Recovery.** A damaged `CURRENT` makes reads and writes return `vault_recovering`. `recovery_report` lists publish-journal evidence first, then complete but never-published manifests; nothing is chosen automatically. `adopt_recovery_point` needs the owner and a complete candidate and writes a receipt before replacing `CURRENT`.
- **Backup exit.** A pinned-commit export (descriptor, whole chain, every cataloged revision, head objects, sealed audit segments) is verified independently and restored only into an empty verified root, with network use gated until reconciliation. Encryption is restic's job; the adapter passes the password only through a cleared child environment.
- **Audit and health.** Audit lines are validated `AuditEvent`s in size-rotated JSONL segments; health reads never write.
- **Measured cost and trigger.** A complete catalog costs about 241 bytes per record in every manifest, so cumulative manifest bytes grow quadratically with single-record commits, and full-history validation grows with the Vault. MV-1 keeps this for simplicity and verifiability. The segmented or incremental catalog (and incremental validation) is required before a head manifest exceeds 1 MiB (about 4,000 records) or commit latency exceeds 1 s on the reference machine, whichever comes first; real history import (MV-2) must be measured against this trigger.

### ADR-MEM-38 — ImportManifest as a revisioned record (Adopted, MV-2)

Each user-selected import is a revisioned `import` record (`imp_…`) stored at `vault/raw/manifests/<id>/<revision>.json`, as the layout specified. A new revision is committed at each step (archived, every parse batch, completion) in the same transaction as the sources that step produced, so the resume cursor, adapter version, counts, and coverage always describe exactly what that commit contains. It records the input kind, the received bytes' hash and size, the adapter and observed source schema, sanitized archive members with their disposition (parsed, preserved only, quarantined, skipped), the cursor, counts, per-conversation coverage (upstream IDs, counts, times; never titles), warnings, and `duplicate_of` for a byte-identical re-import.

Closure rules: an imported source names an existing import and cites that import's received bytes (`source.import_unresolved`, `source.import_raw_mismatch`); a completed import's coverage counts exactly the sources citing it (`import.coverage_mismatch`); an import past `archiving` is committed only with its Raw object present (`store.import_raw_missing`). Raw completeness and parse completeness are separate: an unrecognized format is archived and ends `partial` with no adapter. The synthetic fixtures now carry an import manifest and its raw bytes for every imported source.

### ADR-MEM-39 — Segmented catalog and scoped commit validation (Adopted, MV-3.0)

MV-2 crossed the ADR-MEM-37 trigger: 4,000 imported sources made a 1.1 MB head manifest, and one assertion from a fresh process after that import took 0.78 s, growing with the Vault. Two changes replace the complete catalog and the full-history validation. The logical model does not change: `read_manifest` still returns the complete `CommitManifest` v1 view, and `set::validate_set` still defines consistency.

**Stored manifest and segments.** A commit is stored as a format-2 manifest (`contracts/store/stored-commit-v1`): the v1 header, epochs, review and tombstone IDs, and receipt, plus references to catalog segments instead of inline `catalog` and `objects`. Segments (`contracts/store/catalog-segment-v1`, `vault/catalog/<sha256>.json`) are immutable and content-addressed; an unchanged segment is reused by hash, so a commit writes only the segments it changes.

- Record segments are per record kind and partition that kind's IDs by the hex digits after the ID prefix (dashes removed). The partition is a pure function of the ID set and the manifest's `segment_capacity` (512): a prefix is a leaf when it holds at most the capacity and its parent holds more. Object segments partition object hashes the same way.
- A record entry carries the latest revision's hash and the hashes of every earlier revision, so the head alone names every document ever cataloged (what validation and export need); `Vault::read_revision` serves any revision the pinned entry names. `verify` now checks every named revision and reports damaged segments by hash. It also carries the record's group IDs across all revisions: the imports a source cited, the session of an event. Identity Markdown object entries name the identity revision they are stored beside, which replaces the search back through the chain.
- The descriptor's `format_version` becomes 2, and so does a pinned export's `export_format` (it now carries stored commits and segments). Format 1 was never used for real data; this build refuses a format-1 Vault with `unsupported_schema` instead of carrying two read paths.

**Scoped validation.** Each commit is validated by `delta::validate_delta` on a scoped set: every record of the small kinds (all revisions), and of the bulk kinds (sources, attachments, session events) only what `delta::delta_needs` lists: every revision of each changed record, the documents the delta's rules read, the whole group when a group rule applies (all sources citing an import completed at the head or in the delta; all events of a session the delta touches), and the needs of every small record whose rules read a delta document. The same rules run on the scoped set without and with the delta; a commit is refused if any `(rule, path)` occurs more often with it. A rule instance that reads no delta document cancels out, however incomplete its inputs; one that reads a delta document has all its inputs, so it yields the whole-Vault result. Premises: bulk records are read only by exact revision, by ID, or by group; rules over bulk records that read small ones only get more satisfied when records are added (policy references); and an import revision may not change the received bytes (`import.input_changed`, new). `tests/delta.rs` checks the equivalence for every record, every commit, and every copied or next-revision record of all synthetic sets and mutation cases, and seeding a gap into `delta_needs` makes it fail. The chain step (`set::check_commit_step`) is checked separately; catalog closure is the store's construction.

Measured (`measure_import`, synthetic, release build, same machine as MV-2): 4,000 imported sources now leave a 6 KB stored commit after one assertion (was 966 KB), and that assertion takes 0.05 s from a fresh process (was 0.71 to 0.78 s). 20,000 sources: 83 source segments, a 22 KB stored commit after one assertion, 0.06 s. Import time is unchanged within the noise of this machine (it is file creation, about 3 ms per source). Receipts still list every record a commit changed, so a large batch has a proportionally large stored commit.

Cost and trigger: small kinds are still loaded completely per commit (cached in a long-lived process), and a group rule loads its whole group (an import's completion commit reads that import's sources; a session append reads that session's events). If memories, candidates, and reviews together exceed about 20,000 revisions, or a group load exceeds 1 s, the next step is per-group indexes for those kinds.

### ADR-MEM-40 — Governance: candidates and plan-bound owner review (Adopted, MV-3.1–3.3)

`enouia-memory-govern` composes the contract and the Vault; it writes only through `Vault::commit`.

- **Candidates.** Any principal with the propose right stores a pending candidate. Evidence is resolved from the cited source revision: the evidence class, anchor object, and default locator come from the source, so a proposer cannot present a model claim as the owner's statement. Sensitivity defaults to, and may not drop below, the strictest cited source. A proposal against a stale target revision is refused. The fingerprint (proposal kind, type, content without whitespace, details, target) finds a pending duplicate instead of storing it twice. Active memories that make a different claim in the same scope (fact: claim key; preference: scope; with shared subjects) are listed as conflicts. Only the proposer or the owner may edit (new pending revision) or withdraw a candidate. Proposed fields are limited to a whitelist; IDs, status, evidence, approval, and policy are decided by the review.
- **Plan and confirm.** `plan` turns the owner's decisions into every record the commit would write, generates the review, memory, conflict-group and commit IDs and a nonce, and hashes the records without their volatile timestamps (and without the hash field itself). The trusted surface shows that plan. `confirm` requires the Vault's genesis owner on the same trusted surface before `expires_at` (10 minutes), rebuilds the plan on the current head with the same IDs, and commits only if the rebuild hashes the same; the commit also carries every candidate and target revision as an expected revision. A batch is one commit: any stale decision refuses all of it. The idempotency key is the nonce, so a repeated confirmation replays the receipt; each review's `approval_nonce` is the plan nonce plus its index, and `approved_diff_hash` is the plan hash.
- **Decisions.** Accept or edit-accept by proposal kind (create; revise with evidence union and the stricter sensitivity; archive; supersede with the confirmed effective time and the old record's next revision superseded), reject (optional reason code), merge into a memory (new revision with the union of evidence) or into a pending candidate (new pending revision). Accepting a create that lists live conflicts puts the new memory and those memories in one conflict group; none wins by time or confidence. Undoing a wrong acceptance is an owner archive or revise proposal and its own review; nothing is rewritten in place.
- **Identity (MV-3.3).** An identity change is a candidate like any other; accepting it writes the next identity revision and its Markdown object, and the plan's diff carries the Markdown text itself, so the owner approves the words. Earlier revisions and their Markdown stay readable (`read_revision`, `read_object`); going back is another reviewed revision.
- **Store guard (MV-3.3).** `Vault::commit` refuses memory, review, identity, project, tombstone, purge receipt, approval, and policy records unless the commit's principal is the Vault's genesis owner (`store.owner_only_kind`), beneath the review rules.
- **Boundary.** The in-process check is the Vault owner principal and the trusted-surface enum. Which process may speak for the owner is transport authentication (MV-8); the MCP host will not expose raw commits. Session-checkpoint memories come from reviewed checkpoints in MV-5.

### ADR-MEM-41 — Forget, purge, and deletion reconciliation (Adopted, MV-3.4)

- **One path in.** Deleting starts as an owner-only delete candidate (details: `mode`, `scope`) and an owner review whose plan shows the tombstone with every target. A delete confirmation is committed alone (`review.delete_alone`), as a `logical_delete` or `purge` operation, and moves the deletion epoch: from that commit the canonical view excludes the targets and pinned reads are refused by `check_fresh`.
- **Forget** (`logical_delete`) targets the memory (one revision or all) and keeps its text, restricted.
- **Purge** targets are computed, never typed: the memory's revisions, the candidates that proposed or targeted it (not the delete request itself), and with `with_dependents` the cited sources and the raw objects holding them. `purge_preview` lists the same, plus other memories that would lose evidence and how many other sources each raw object holds: a message inside an imported export cannot be removed alone, so the whole object is destroyed or kept. Sanitized replacement objects are not implemented.
- **After the tombstone.** The Vault treats what purge tombstones name as intentionally purged: reads return `intentionally_purged`, `verify` counts it as purged (not missing), exports and `verify_export` leave it out, sweeps never quarantine it, and commit validation treats it as absent (rules reading it cancel out; a new record citing it fails). `purge_files` deletes the bytes and any quarantined copy under the writer lock and is idempotent. `complete_purge` then commits a purge receipt: local stores purged or not present, exports `not_manageable` (outside the Vault), backups `pending` with a deadline (30 days), overall `backup_purge_pending`. Nothing claims global erasure. Catalog hashes of purged revisions remain, as the design allows.
- **Deletion ledger (B02).** The ledger is the head's tombstones (IDs, hashes, modes; no text), kept apart from backups. `reconcile_deletions` re-applies each entry to a restored Vault through an owner-confirmed plan (and `complete_purge` for purges), reports what was applied, already covered, or not present, and only then marks the restore reconciled, which opens its network gate.
- **Not yet.** Other memories that lose evidence are listed, not marked `broken`; capsules, indexes, and replicas do not exist yet and are reported `not_present`.

### ADR-MEM-42 — Local index and literal search (Adopted, MV-4)

`enouia-memory-index` keeps a disposable SQLite projection at `indexes/memory.sqlite` and answers `memory_search`.

- **Dependency.** `rusqlite =0.40.2` without default features, with `bundled` (SQLite 3.53.2 compiled from source). Building now needs a MinGW C compiler; with MSYS2 the compiler's own `bin` directory must come before the Rust toolchain's in `PATH`, otherwise `cc1` loads the toolchain's older `libgcc_s_seh-1.dll`. The installed binary needs no extra DLL (tested with only System32 on `PATH`).
- **Projection.** One row per memory revision with the commit sequences between which it was the record's latest revision, plus projects (folded names) and tombstones. The watermark advances with the commits of each transaction (up to 256, cancellation checked between), so a crash or cancellation leaves an older, consistent index; an index of another Vault, format, or folding version, one ahead of the Vault or on another chain, or one that fails `quick_check` is refused and rebuilt. Journal mode `DELETE`, `secure_delete` on; a purge removes the rows and their FTS entries, merges the FTS segments, and vacuums, and a rebuild never reads purged content.
- **Folding `fold-1`.** Full-width ASCII to ASCII, ideographic space to space, lowercase. No NFKC (no tables offline) and no simplified/traditional or Japanese variant conversion: 函馆 and 函館 stay distinct. Indexed text is what the owner reads (title, content, tags, category, descriptive fields); internal keys such as claim keys are not full text.
- **Recall.** The query is literal: folded, split on whitespace, at most 16 terms. Terms of three or more characters go to an FTS5 `trigram` table, each as a quoted phrase with quotes doubled; shorter terms are a bounded `instr` scan (projects first); all terms must match. ASCII words are also looked up in a `unicode61` table. A term equal to a project's name or alias recalls that project's memories; projects that only contain a term are reported as ambiguous and nothing is merged. Each path takes at most 500 candidates; beyond that the page is `partial`.
- **Filters, before anything is returned.** Requested types and projects, every current tombstone (for any snapshot), broken provenance, the principal's `memory:read` decision against the current policies (projects the principal cannot read are not even named), and time: `currency` at `as_of`, historical-only facts only on request, archived and not-yet-effective ones never.
- **Ranking `rank-1`.** Ascending key: named-project hit first, then currency (current, needs reverification, conflicted, historical), whole-word hit, text-path position (bm25 or scan order), priority, business date (valid from, observed, created; newest first), memory ID, revision. No raw scores are added. The golden set in `tests/fixtures/memory/search-golden.json` freezes it; any change needs a new version and new expectations.
- **Consistency.** A search needs the index at the Vault's head (`index_not_ready` otherwise; the caller updates or offers the source browser). The returned page is re-read from the Vault and must equal the indexed revision. A cursor binds the snapshot sequence, both epochs, the principal, the filters, and the ranking version; a later commit does not move its pages, a deletion or policy change invalidates it. `known_at` selects the rows known at an earlier commit, still under the current tombstones and policies.

### ADR-MEM-43 — Local context, sessions, and the offline Mock (Adopted, MV-5)

`enouia-memory-context` compiles context capsules, keeps durable sessions, and answers through a deterministic local Mock.

- **Saved requests are records.** Capsule, inspection, and dispatch are single-revision records in the commit catalog (`records/capsule|inspection|dispatch/<id>.json`); what the Mock (and later a Provider) receives is re-rendered from the saved capsule and must match the dispatch hash. There is no second, hidden context path. A purge deletes the saved requests and derived replies that name purged records.
- **Compiler `context-1`, ranking `context-rank-1`.** Recall through the index at the Vault's head (`index_not_ready` otherwise, never a hidden scan); hard exclusions first (tombstones, `memory:read`, `highly_sensitive`); currency at request time; whole items only, never truncated; the budget is the rendered request's UTF-8 bytes plus a fixed wrapper (`mock-utf8-1`) until MV-7 fixes real tokenizers. A decided project state without implementation evidence is flagged `implementation_unverified`. The commit is made only if epochs are unchanged since the pin.
- **Sessions.** User input is committed before compile or answer; output is chunk events ending in completed, cancelled, or failed; a checkpoint is provisional and covers an ordered list of event IDs with a `coverage-1` hash (SHA-256 of the canonical `[{event_id, sequence, kind, content_hash}]`). Every write is idempotent under its request key.
- **Mock.** No network, tools, or model. Deterministic statements with source references, or abstention (`no_supported_evidence`, `implementation_unverified`, conflict). Source text is data under fixed system rules.

### ADR-MEM-44 — Windows Memory Workspace: embedded Core, typed workspace IPC, Tauri shell (Adopted, MV-6; hosting amended by ADR-MEM-45)

The Windows workspace is two layers: `enouia-memory-workspace` (the embedded Core: commands, long operations, review plans, picked-file tokens) and `apps/workspace` (a Tauri 2 shell with a React/TypeScript frontend built by Vite). The shell only forwards; every rule lives in the Core crate, which is tested without a window.

- **One typed channel.** The frontend calls one Tauri command, `workspace_call`, with a [workspace IPC v1](../../contracts/ipc/workspace-v1.schema.json) envelope (`schemaVersion: 1`, `requestId`, `command`, `idempotencyKey`, `arguments`) and gets `{schemaVersion, requestId, kind, vaultCommitId, operationId, result, error}`. Errors are `{code, retryable, rules}`: contract codes and rule identifiers only, never paths, OS messages, or record text. Write commands need an idempotency key; a cancelled or failed operation never reports `succeeded`.
- **No paths from the page.** Files and folders are chosen in native dialogs opened by the shell (`rfd`); the Core turns the choice into a single-use token (`tok_` + 32 hex, bound to its picker kind, 10 minutes) that names only the file's base name and size to the page. The frontend has no `fs`, `shell`, `http`, or dialog plugin, a strict CSP (`default-src 'self'`, no remote images, no inline script), and only the app's own commands in its capability.
- **Owner on a trusted surface.** As with the CLI, the owner is the principal that created the Vault and the surface is `trusted_windows_app`; MV-8's Host will authenticate it. A review is `review_plan` (the exact records the commit writes and their diff hash, kept by the Core for `PLAN_TTL`) then `review_confirm` with the plan ID and the shown diff hash; any other hash, an expired plan, or a moved head writes nothing. Forget is the same plan path with a delete candidate. Remember and correction create a manual-assertion source and a pending candidate, never a memory.
- **Long work off the UI thread.** Import, resume, index rebuild, Vault verification, and backup export run on worker threads and return an `operationId`; `operation_get` reports `queued/running/succeeded/failed/cancelled` and progress (commits applied or batches committed; `total` is null when unknown). Cancellation is checked between commits or batches, so a cancelled import is resumable and a cancelled rebuild leaves a consistent, older index. While the index is busy, searches return `index_not_ready` immediately instead of waiting, and the status shows `memory_index: recovering` with the Vault still healthy. Index rebuild gained a cancellable variant (`Index::rebuild_with`).
- **Four different stops.** Closing the window hides it (operations continue, tray stays). Lock Vault cancels and joins operations, then drops the Vault and index handles; every Vault command answers `vault_locked` until unlocked. Exit does the same and ends the process. Sync does not exist before MV-9, so "pause sync" is shown as not available. Activity is shown as independent and is never started, stopped, or read.
- **Data root.** Only an explicitly chosen root (`--vault <dir>` or the folder picker) is opened; a new Vault needs the owner to type the confirmation phrase. There is no default location. The pinned installer template omits AppData deletion and unscoped Run-value removal: the uninstaller owns application files, never inferred Vault locations. See [uninstall evidence](../validation/MV-6-uninstall.md).
- **Companion shell.** A tray icon (show, lock, exit), a global hotkey (Ctrl+Alt+M by default, another letter with `--hotkey-key`) registered with `RegisterHotKey` on its own thread, reported as `registered` or `conflict` instead of silently failing, and a small always-on-top quick-search window. Its native, injected window identity restricts `workspace_call` to `memory_search`; its separate capability permits only that channel and show/hide, with no file picker, exit or startup command. Page-supplied window/principal fields cannot widen the channel. Lock hides the overlay; focus/blur clear its transient query/results and invalidate old reads. See [overlay evidence](../validation/MV-6-overlay.md).
- **Rendering.** Source and memory text are rendered as plain text nodes. No Markdown/HTML rendering, no `dangerouslySetInnerHTML`, no link navigation out of the app.
- **Real-app check.** `apps/workspace/e2e/smoke.mjs` drives the release binary over WebView2 remote debugging on loopback (enabled only by that test's environment) and fills the native dialog of that process only; see the [MV-6 report](../validation/MV-6.md).
- **MV-6 follow-up.** A current-user NSIS installer owns app files, not user-selected Vaults. Explicit opt-in startup is shell-local, restricted to the main window and one fixed Run value; no Vault root is persisted or automatically opened. Background launch hides the window before creation. These device settings do not extend the Memory IPC contract or introduce Runtime/Activity dependencies. The separately built retry fixture and opt-in renderer crash drill exist only in synthetic debugging runs; no production fault command or permission is added. See the [follow-up evidence](../validation/MV-6-followup.md).

### ADR-MEM-45 — Runtime hosts Memory's local client (Adopted, Runtime integration; one-Core rule enforced by ADR-MEM-46)

Refines ADR-MEM-19 and amends the hosting part of ADR-MEM-44. The handoff, host duties, change routing, and compatibility log are in [integration/RUNTIME.md](../integration/RUNTIME.md).

- **Product client.** Enouia Runtime's Windows client (`Morii9961/enouia-runtime`, `apps/desktop`) is the product client for Memory's local part. It embeds `enouia-memory-workspace` at a pinned Git revision (a full commit SHA, never a branch; no path dependency or `[patch]` in committed manifests) and adds its own adapter: native pickers that return tokens, window scoping, and lifecycle. Memory still never depends on Runtime and builds and tests alone.
- **One seam.** A host calls only `Workspace` (`new`, `call`, `register_pick`, `open_root`, `set_companion`, `shutdown`) with workspace IPC v1 envelopes. It never calls the domain crates directly. When MV-8 replaces the embedded Core with the single Memory Host, the envelope stays and only the host transport changes; that switch is a logged surface change with a Runtime migration.
- **Shared scope rule.** `HostSurface` (`Workspace`, `QuickSearch`) in `enouia-memory-contract::workspace` is the one caller scope. Hosts derive it from their native window identity and check it before forwarding. The reference shell maps `main`/`overlay` onto it; Runtime maps its own windows. Page-supplied fields never widen it.
- **Reference shell.** `apps/workspace` stays here as the reference shell and acceptance harness: MV-6 evidence, installer tooling, and later MV-8.1 reconnect tests. It is not the product client. New product UI for the local part lands in Runtime first; changes to the reference shell or frontend are logged for Runtime like any other surface change.
- **Integration surface.** `docs/integration/runtime-surface.json` records every file Runtime depends on (wire contract, Core crate, build closure, reference shell and frontend, and since 2026-10-06 the reference installer that Runtime's installer derives from) with its digest. `crates/enouia-memory-contract/tests/runtime_surface.rs` fails until a change is regenerated with `tools/integration/runtime_surface.py` and its aggregate is logged in `integration/RUNTIME.md`. The test forces an acknowledgment in this repository; it cannot prove Runtime adopted the change.
- **Trusted surface.** Runtime-hosted reviews are stamped `trusted_windows_app`. That holds only while the host keeps the W04 guarantees: strict CSP, no `fs`/`shell`/`http`/dialog plugins, plain-text rendering, no paths from the page, and no logs of request or response bodies.
- **One Core per Vault.** The reference shell, Runtime, and the CLI can each open the same Vault. Commits serialize through the writer lock, but two embedded Cores also hold the index file. Run one embedded Core per Vault at a time until MV-8's Host owns the lock; ADR-MEM-46 enforces this.
- **Data root.** Hosts open only an explicitly chosen root (ADR-MEM-37, 44). The `%LOCALAPPDATA%\EnouiaMemory` default named in ADR-MEM-19 is not used. A remembered or default root in any host needs a Memory ADR.
- **Cloud stays here.** MV-7 (real Provider), MV-8 (Host, MCP), MV-9 (gateway, queue), MV-10 (replicas), MV-11, `contracts/ipc/memory-v1.schema.json` (the agent surface), `contracts/provider/capabilities-v1.schema.json`, backup, and their code and ADRs remain in this repository. Runtime implements no Provider calls, MCP, gateway, or replica, and never carries Memory sync over its Activity delivery path.
- **Activity.** Activity data never enters workspace IPC. A host replaces the Core's fixed Activity status row (`component: "activity"`, `mode: "independent_not_managed"`) with its own Activity health.

### ADR-MEM-46 — Host-facing Core follow-ups (Adopted, Runtime integration)

These were found while handing the local client to Runtime (ADR-MEM-45). They change how the workspace Core answers its hosts. The workspace IPC v1 envelope and commands are unchanged, and every change is additive for pages.

- **Root reasons.** A rejected data root answers `invalid_request` with `workspace.root_rejected` plus one reason rule. The reasons are `root.inside_repository`, `root.cloud_sync_folder`, `root.not_local_fixed_disk`, `root.unsupported_filesystem`, `root.system_location`, `root.reparse_point`, `root.not_canonical`, `root.network_path`, `root.missing`, `root.not_a_directory`, `root.insufficient_space`, `root.unreadable` and `root.not_absolute`. The path is never returned.
- **Tokens survive failures.** `vault_open`, `vault_create`, `import_start`, `backup_export` and `restore_preview` consume their picker token only when they succeed. A failed attempt can be retried with the same choice until the token expires.
- **Confirm retries.** `review_confirm` keeps its plan after a retryable failure, for example `busy` while another writer holds the lock. The retry replays idempotently through the plan nonce's receipt, and a purge's file step is keyed. A non-retryable failure ends the plan, as before.
- **Early cancels resume.** An import cancelled before its first batch stays `archived` (received, not parsed). Its operation now reports `cancelled` and resumable, as it already did for `parsing`, and the reference shell offers Resume for both states.
- **Keyed operation starts.** `import_start` and `import_resume` honour their idempotency key within the process. The same key and request answer the operation already started. The same key with a different request gets `idempotency_conflict` / `workspace.key_reuse`. Keys reset when the Vault closes.
- **One Core per Vault.**
  - While a workspace Core has a Vault open, it holds `indexes/host.lock` exclusively. The lock file sits with the disposable index, never with canonical records.
  - A second Core, meaning another host process, gets `busy` / `workspace.vault_in_use`. It is not retryable, because another app holds the Vault.
  - The lock belongs to the open/close lifecycle, not to in-flight handles. `close` detaches the Vault before it cancels and joins operations, so no command can start work on a closing Vault. It then unlocks explicitly. The OS releases the lock if the process ends.
  - A refused unlock leaves the Vault locked, so it can be retried once the other app lets go. Unlocking a Vault that is already open answers its status.
  - The CLI commits through the per-commit writer lock and does not keep the index open, so it is unaffected.
  - MV-8's Host will replace this with its own exclusive lock.
- **Contained failures.** A panicking operation ends `failed` with `storage_failed` / `operation.panicked` (not retryable), even after a cancel request, instead of staying `running`. A poisoned index is dropped and reopened from the Vault, since the index is disposable.

Evidence: the workspace Core tests `rejected_roots_name_their_reason_and_keep_the_token`, `a_busy_confirm_keeps_its_plan_for_the_retry`, `an_import_retry_answers_the_started_operation`, `a_second_core_cannot_open_an_open_vault`, `a_held_handle_does_not_block_this_cores_reopen`, `unlock_of_an_open_vault_answers_at_once`, `a_panicking_operation_ends_failed`, `a_panic_after_cancel_still_ends_failed` and `an_import_cancelled_before_its_first_batch_is_resumable`. The reference shell names `workspace.vault_in_use` and the `root.*` reasons in owner-readable text.

- **Vault-scoped page state (2026-10-05 follow-up).** Lock, root switch, a failed open after close, and shutdown cancel/join workers before forgetting operation history, pending/confirmed plans, import retry entries, `lastBackup`, and import/backup/export picker tokens. Root picker tokens retain their existing expiry and success-only consumption, so a rejected open/create can still be retried. Progress remains readable during the join; old operation IDs become `workspace.operation_unknown` afterwards. A confirmed-plan replay first requires an open Vault and the original diff hash. Same-diff retries within one open Vault still return the original receipt. Stored records and backups are unaffected. See [lifecycle evidence](../validation/MV-6-vault-lifecycle.md).

- **Core lifecycle exclusion (2026-10-05 follow-up).** A transport-neutral read/write gate protects ordinary request processing and response commit binding, native picker registration, native open and shutdown. Open/create/lock/unlock/shutdown are exclusive; ordinary calls share the read side. Status and task reads/cancellation bypass it to stay usable while close waits for in-flight calls and joins workers. This removes reliance on host serialization for individual Core call safety; hosts retain their UI/action sequencing duties. A poisoned lifecycle writer refuses new actions with non-retryable `workspace.lifecycle_failed`; shutdown attempts cleanup, then the host recreates the Core. See [concurrency evidence](../validation/MV-6-core-concurrency.md).

- **Cancellation is an intent (2026-10-05 follow-up).** Worker success stays successful after a cancellation request; unrelated worker errors stay failed; `MemoryErrorCode::Cancelled` or the operation's explicit cancelled result reports cancellation. Cancellation admission and terminal publication share the outcome mutex, so a late request does not rewrite a completed task's flag. Close requests cancellation only from queued/running tasks before joining. See [outcome evidence](../validation/MV-6-operation-outcomes.md).

- **Reference-host lifecycle scheduling (2026-10-05 follow-up).** The tray queues lock/exit on blocking workers, and page exit awaits a shutdown worker. The exit callback runs only after Core cleanup finishes. The native event callback does not wait for in-flight calls or long operations. This uses the reference shell's existing Tauri worker pattern and changes no Core/wire API. Runtime must use and validate its own equivalent scheduling. See [scheduling evidence](../validation/MV-6-shell-lifecycle.md).

- **Picker admission (2026-10-05 follow-up).** A token is reserved while its command validates or schedules work. Concurrent reuse reports retryable `busy` / `workspace.token_busy`; other tokens proceed independently. Errors and unwind release the reservation; successful consuming commands remove the token. Import preview remains reusable. Scheduling success consumes import/backup tokens even if the worker later fails. Expiry remains ten minutes from selection, and root rejection retries remain valid until expiry. No picker mutex is held during command IO. See [picker evidence](../validation/MV-6-picker-admission.md).

- **Excerpt pagination (2026-10-05 follow-up).** `source_excerpt` refuses an insufficient UTF-8 byte budget with non-retryable `invalid_request` / `workspace.excerpt_budget`, instead of returning an empty nonterminal page. A successful page before EOF advances its cursor and respects the byte cap. UTF-8 start rounding and empty EOF behavior stay unchanged. Hosts may use a budget of at least four bytes, within the existing cap, to fit any single UTF-8 character. See [pagination evidence](../validation/MV-6-excerpt-pagination.md).

- **Remember replay binding (2026-10-05 follow-up).** The workspace checks both text and `claimKey` against the original candidate revision named by an existing remember receipt, before and after proposal admission. Different input with that key reports `idempotency_conflict` / `workspace.key_reuse`; identical retries remain valid after review or reopen. The receipt and historical read use the existing storage format and purge barriers. This is a Core `remember` guard, not a general redesign of governance idempotency. See [replay evidence](../validation/MV-6-remember-replay.md).

- **Concurrent manual assertion replay (2026-10-06 completion).** The service now returns the source ID in the original receipt when commit-time replay wins after concurrent initial lookups. A deterministic Core helper regression reproduces the discarded-ID return and verifies the correction. No store format or writer protocol changes. Other service returns and full-page concurrent remember remain separate checks. See [finding and correction](../validation/MV-6-pending-source-replay.md).

- **Concurrent Context session replay (2026-10-06 follow-up).** Session start, event append, fork and checkpoint restore IDs from the published receipt on commit-time replay. Session/branch reconstruction uses the receipt's historical revision; subsequent forks do not alter an original retry result. Ordinary committed writes, existing hashes and store preconditions stay intact. The deterministic tests include actual concurrent `session_new` page calls and direct input/fork/checkpoint service calls. See [session evidence](../validation/MV-6-session-replay.md).

- **Mock dispatch/reply replay (2026-10-06 follow-up).** Initial-lookup and commit-time replay restore the published dispatch ID/hash before reply saving, through the current catalog and original receipt revision. Restore rechecks capsule/request binding and policy/deletion watermarks. Deterministic full-page calls cover late receipt visibility, commit replay, fresh transport IDs on retry and deletion while replay waits. The dispatch/reply pair remains separate commits; no external provider is introduced. See [Mock replay evidence](../validation/MV-6-mock-replay.md).

- **Correction/forget request binding (2026-10-06 follow-up).** Core checks published proposal receipts before/after admission, reading the original candidate revision. Corrections bind target/revision/text; forgetting binds target/mode/dependent scope. Changed arguments conflict with no result; existing stale/review checks remain. The shared reader preserves remember's text/claim binding. This does not establish domain-wide binding or atomic source/proposal writes. See [proposal-binding evidence](../validation/MV-6-proposal-binding.md).

- **Full session ask replay acceptance (2026-10-06 follow-up).** Two synthetic page schedules start before input/capsule creation, with identical keyed arguments and distinct transport request IDs. They verify one completed turn, a published/verified dispatch and saved reply, subsequent replay without head movement, and changed-text refusal. Bounded contention retries are test-only. This extends the earlier service/saved-state evidence without changing production code or establishing every concurrency schedule. See [full session replay evidence](../validation/MV-6-full-session-replay.md).

- **Late owner proposal receipt recovery (2026-10-06 follow-up).** Core handles a candidate-byte idempotency conflict by checking the original published logical proposal, then making one bounded receipt lookup retry. Remember/correction/forget share this helper; route-specific guards remain. Generated IDs no longer prevent the tested identical late remember replay, while changed claims are refused. Existing persistent hashes and writer behavior stay intact. See [admission evidence](../validation/MV-6-proposal-admission.md).

- **Different-key pending proposal deduplication (2026-10-06 follow-up).** New governance candidate publication carries the head used for its pending-fingerprint lookup. A stale head is refused by the existing writer precondition; Core re-evaluates once on that exact fault. Identical proposals deduplicate and distinct ones remain eligible. Direct governance callers now receive an optimistic head conflict; edit/withdraw preconditions and persistent hashes are unchanged. See [dedupe evidence](../validation/MV-6-proposal-dedupe.md).

The [full remember replay and target freshness acceptance](../validation/MV-6-full-remember-replay.md) extends ADR-MEM-46's synthetic evidence through initial source creation and a correction target revision changed during proposal assembly. It changes no production behavior or contract; actual Runtime acceptance remains separate.

- **Import preview input admission (2026-10-06 follow-up).** Core revalidates the selected file kind at token use and bounds reading by the observed length and existing input budget, refusing changed or oversized input with existing import rule names. Failed previews retain the token. Actual reparse races and the import writer remain separate gates. See [preview evidence](../validation/MV-6-import-preview-input.md).

- **Background import read budgets (2026-10-06 follow-up).** The existing Core import worker's domain pipeline bounds both selected-file reads by observed/global size, while preserving exact double-read/hash admission before archive publication. Controlled actual file replacements cover both open boundaries. See [worker read evidence](../validation/MV-6-import-read-budget.md).

- **Complete confirmation result replay (2026-10-06 follow-up).** Core holds its existing result-cache mutex through confirmation and optional file purge. Concurrent confirmation retries replay the complete first result instead of overwriting removal counters. Confirmations within one Core serialize; existing retryable-plan retention and status observation remain. See [confirmation evidence](../validation/MV-6-confirm-result-replay.md).

### ADR-MEM-47 — Explicit text Provider dispatch (Adopted implementation; real activation pending)

[Decision](047-explicit-provider-dispatch.md): native OpenAI Responses/Anthropic Messages adapters, owner-inspected egress policies and exact body approvals, bounded calls and no automatic resend, durable Session invocation ledger, derived sensitivity, and payload deletion. Existing Mock/page IPC remains unchanged. Runtime adoption and real-account smoke are separate gates; optional extraction is unimplemented.

### ADR-MEM-48 — Hosting native Provider dispatch in the workspace Core (Adopted implementation; real activation pending)

[Decision](048-workspace-native-provider-hosting.md): the host injects a native transport, secret store and owner-verified destinations with `Workspace::set_native_provider`; seven workspace IPC commands cover grant/revocation plans, exact-body preparation, one-attempt sending as an operation and local interruption. Without the injection the Core stays Mock-only. Close/lock disable native access before joining workers. Amends ADR-MEM-47's "workspace stays on Mock" for hosts that opt in.

### ADR-MEM-49 — Cloud primary and historical initialization (Adopted direction; H1 implemented)

[Full decision](049-cloud-primary-and-history-initialization.md) and [design v1.3](../design/CLOUD_PRIMARY_AND_HISTORY.md). Target one cloud primary with local initialization, offline copies and independent encrypted recovery. H1 implements only a pure read-only history planner and a synthetic example. This changes future Host/cloud/replica targets, not current workspace IPC, storage, approval or Provider behavior. Cloud trust scope, actual data/calls/deployment and automatic acceptance remain unactivated. Later MV stages still need explicit starts.

### ADR-MEM-50 — Archived history extraction batches (Adopted, MV-7-H2a)

[Decision](050-archived-history-extraction-batches.md): bind explicitly selected H1 batches to archived bytes, adapter and current source revisions; save a source-linked coordinator input, then create existing extraction jobs with distinct replay keys. Reopening/cancellation/publication failure reuse those jobs without a transport call. A changed plan under the same owner key conflicts. Child invocation failures and unknown outcomes remain visible and never authorize resending. No new Session fields, record kinds, wire commands or automatic acceptance. [Evidence](../validation/MV-7-history-binding.md).

### ADR-MEM-51 — History review and explicit supersession (Adopted, MV-7-H2b)

[Decision](051-history-review-and-explicit-supersession.md): compare completed saved Fact/Preference suggestions by exact topic keys while retaining dates, source context, branch relationships, original suggestions and current owner edits/status. Repeat/difference hints do not select a current truth. The owner chooses a pending candidate, active target revision and business time to obtain a read-only Supersede draft; publication uses a head-bound proposal and ordinary trusted plan/confirmation. No new persistence, wire command, automatic acceptance or topic summary. [Evidence](../validation/MV-7-history-review.md).

### ADR-MEM-52 — Source-linked topic briefs (Adopted, MV-7-H2c)

[Decision](052-source-linked-topic-briefs.md): retain per-run selected source coverage, uncited/zero-candidate slices and response/application metadata. Produce a bounded Chinese Markdown navigation brief in memory, with exact topic filtering, source-linked original suggestions and separate current candidates. Imported text stays fenced JSON data. Extraction rendering supplies source text once while retaining stored Turn text and dependencies; path/secret guards remain. This does not synthesize or approve topic summaries. [Evidence](../validation/MV-7-topic-briefs.md).

### ADR-MEM-53 — Source-bound semantic history review (Adopted, MV-7-H2d)

[Decision](053-source-bound-semantic-history-review.md): persist an exact-topic, source-linked ordinary Session task; explicitly prepare/send through existing native Provider admission with an AnalysisGuard. Whole-response validation resolves exact quotes for proposed changes and summary points. Corrections/cancellations require user evidence and directed source ancestry; derived interpretations never publish or approve memories. Latest source revisions, immutable original responses, uncited coverage and replay remain explicit. [Evidence](../validation/MV-7-semantic-review.md).

### ADR-MEM-54 — Offline history quality assessment (Adopted, MV-7-H2e)

[Decision](054-history-quality-assessment-harness.md): calculate item faithfulness and required-unit coverage from explicit human judgments bound to selected source/response JSON hashes. Keep contradicted/unsupported, missing and unknown/unreviewed separate; native validation and overall model quality remain unclaimed. Synthetic positive/negative/pending cases calibrate the evaluator. No Core, wire, persistence or Runtime adoption change. [Evidence](../validation/MV-7-quality-assessment.md).

### ADR-MEM-55 — Owner-selected semantic proposal drafts (Adopted, MV-7-H2f)

[Decision](055-owner-selected-semantic-proposal-drafts.md): bind explicit owner choices to saved semantic responses, retain model attribution and original evidence classes in ordinary pending proposals, and require current exact-topic targets and explicit business time for correction/cancellation drafts. Drafting never writes/sends/confirms. Source purge also removes dependent candidate text. [Evidence](../validation/MV-7-semantic-proposal-drafts.md).

### ADR-MEM-56 — Bound semantic proposal publication (Adopted, MV-7-H2g)

[Decision](056-bound-semantic-proposal-publication.md): native publication binds the complete owner choice to existing durable receipts; changed committed keys conflict, replay returns the winning candidate ID, and recovery revalidates sources/current candidate after owner review or reopening. DuplicateOf stays a non-write result. Trusted confirmation remains separate. [Evidence](../validation/MV-7-semantic-publication.md).

### ADR-MEM-57 — Native quality review bundles (Adopted, MV-7-H2h)

[Decision](057-native-quality-review-bundles.md): generate bounded private assessment suites in memory from validated saved semantic tasks, preserve native selected spans/uncited coverage, and match offline JSON value hashes. Pending reviewers have no judgments or acceptance claim; offline metadata stays unauthenticated. [Evidence](../validation/MV-7-quality-bundles.md).

### ADR-MEM-58 — Selected-source lineage input (Adopted, MV-7-H2i)

[Decision](058-selected-source-lineage-input.md): optionally include bounded direct parent indices from selected Source revisions, preserve missing/unknown ancestry and legacy tasks, and separate native Request IDs. Normal native validation and explicit sending remain required. [Evidence](../validation/MV-7-semantic-lineage.md).

### ADR-MEM-59 — Opt-in analysis input in private quality bundles (Adopted, MV-7-H2j)

[Decision](059-quality-bundle-analysis-input.md): optionally retain exact bounded native analysis input with private pending review material. The offline evaluator checks input hash and selected scope agreement without authenticating metadata, re-running native validation or judging model quality. [Evidence](../validation/MV-7-quality-input.md).

### ADR-MEM-60 — Bounded strict JSON for offline quality assessment (Adopted, MV-7-H2k)

[Decision](060-bounded-quality-json-reader.md): reject non-finite/overflowing numbers and over-depth JSON before assessment, retain encoding and quoted-text compatibility, and validate legacy archive/message shapes. Malformed input yields ordinary diagnostics without metrics or tracebacks. [Evidence](../validation/MV-7-quality-json.md).

### ADR-MEM-61 — Explicit collection shapes before offline assessment (Adopted, MV-7)

[Decision](061-quality-collection-shapes.md): require array/object shapes and nonblank string case identities before offline metrics; reject truthiness/iteration substitutes without coercion or partial CLI output. Valid scoring and unknown states remain unchanged. [Evidence](../validation/MV-7-quality-collections.md).

### ADR-MEM-62 — Offline quality review worksheet (Adopted, MV-7)

[Decision](062-offline-quality-review-worksheet.md): render an explicitly selected, validated quality suite as a bounded private Markdown worksheet, with selected sources, response paths, required units, existing judgments and optional structured task input. Preserve pending/unknown states; no auto-grading, file save, Provider or Runtime UI. [Evidence](../validation/MV-7-quality-review-worksheet.md).

### ADR-MEM-63 — Offline review revision comparison (Adopted, MV-7)

[Decision](063-offline-review-revision-comparison.md): compare explicitly selected, fully validated review revisions while binding all non-review input; report added/edited/removed judgment locations and before/after states without repeating source or judgment text. No merge, auto-grading or approval. [Evidence](../validation/MV-7-review-revision-comparison.md).

### ADR-MEM-64 — Saved-analysis lineage shape (Adopted, MV-7)

[Decision](064-analysis-input-lineage-shape.md): share the existing native-matrix lineage structure gate with the ordinary evaluator for optional saved complete input; preserve missing legacy metadata and null unknowns. Shape checking does not authenticate ancestry or model quality. [Evidence](../validation/MV-7-analysis-lineage-shape.md).

### ADR-MEM-65 — Recorded context source dependencies (Adopted, MV-7)

[Decision](065-recorded-context-source-dependencies.md): record selected context dependencies separately from supporting evidence, propagate them through governance/compilation and source purge; explicit optional storage fields preserve legacy reads without claiming complete legacy lineage. [Evidence](../validation/MV-7-semantic-context-dependencies.md).

### ADR-MEM-66 — Host authority and platform transactions (Adopted as contract, MV-8.1a)

[Decision](066-host-authority-and-platform-contracts.md): additive authority/transfer-plan v1 schemas, exact checkpoint-bound quiescence/promotion/abort, monotonic primary epochs and retained bounded migration receipts. Native platform transactions must combine the latest authority check with durable data publication; an unavailable authority never permits cached writing. Pure model and synthetic transaction tests do not migrate existing Vaults, authenticate clients, create a Host process or prove OS/cloud fencing. Runtime still embeds its pinned Core until a later explicit adoption.

### ADR-MEM-67 — Authority-only control persistence (Adopted, MV-8.1b pilot)

[Decision](067-authority-control-file-persistence.md): explicit native Windows control-file persistence with protected owner ACLs, bounded reads, flushed atomic replacement and lifetime host/writer OS leases. Only same-checkpoint quiesce/promote/abort are available; missing/corrupt state never bootstraps on open. This is not the full AuthorityStore data transaction adapter, does not activate existing Core/CLI fencing after the lease ends and is not exposed to Runtime or MCP. [Evidence](../validation/MV-8-authority-control-file.md).

### ADR-MEM-68 — Shared CURRENT publication (Adopted, MV-8.1c library)

[Decision](068-shared-current-authority-publication.md): one CURRENT selects data and immutable authority; fresh data verification and a persistent legacy mutation/export gate. No transport or production activation. [Evidence](../validation/MV-8-shared-current-publication.md).

### ADR-MEM-69 — Workspace history planning (Owner-authorized implementation, MV-7)

[Decision](069-workspace-history-planning.md): expose H1 through one read-only, source-bound workspace command with bounded metadata/pages, opening/head-bound cursors and current SourceRead/deletion checks. No extraction, task or memory write. [Evidence](../validation/MV-7-workspace-history-planning.md).

### ADR-MEM-70 — Workspace local history materialization (Owner-authorized implementation, MV-7)

[Decision](070-workspace-history-materialization.md): persist typed local request admission atomically before offline child creation; add bounded Workspace create/read/resume and Runtime adoption after verification. No sending, credential port, automatic approval, MV-8 or cloud activation. Implementation is in progress.

### ADR-MEM-71 — Saved history batch request preparation (Adopted, MV-7)

[Decision](071-history-batch-request-preparation.md): prepare one ready, uninvoked child derived from a saved history task, bind request keys durably, and reuse exact-body inspection and explicit send without recreating tasks. [Contract and evidence](../validation/MV-7-history-batch-preparation.md).

### ADR-MEM-72 — Saved history batch response application (MV-7)

[Decision](072-history-batch-response-application.md): source-bound response preview and one inspected cursor step into pending candidates, with durable argument binding and restart-safe replay. No sending or automatic approval. [Evidence](../validation/MV-7-history-batch-application.md).

### ADR-MEM-73 — Witnessed Host publication and forward recovery (MV-8.1d)

[Decision](073-host-publication-recovery.md): opt-in native Host v2 binds a publication intent and acknowledgement witness to CURRENT; normal operations refuse incomplete publication. Explicit owner-confirmed recovery verifies the latest exact intent and target bytes, preserves watermarks/receipts and never selects an older commit. Missing ordinary witnesses and prototype v1 roots remain fenced. [Evidence](../validation/MV-8-host-recovery.md).

### ADR-MEM-74 — Native Host domain-writer binding (MV-8.1e)

[Decision](074-native-host-domain-writer.md): an explicit native binding holds the Host lease and captured primary epoch while existing domain commits use witnessed publication. Fresh pins and receipt-only replay also revalidate; legacy handles and maintenance remain fenced. [Evidence](../validation/MV-8-native-domain-writer.md).

### ADR-MEM-75 — Versioned native Host session gate (MV-8.1f)

[Decision](075-native-host-session-gate.md): closed version/schema/Vault handshake and private connection handles with fixed native scope, bounded expiry/capacity, lock/revocation and instance isolation. OS authentication and the process/listener are separate pending adapters. [Evidence](../validation/MV-8-native-session-gate.md).

### ADR-MEM-76 — Host-bound Workspace service (MV-8.1g)

[Decision](076-host-bound-workspace-service.md): native Host sessions dispatch the unchanged envelope to Workspace over the same bound Vault handle. UI lock retains the lease, revokes connections and requires native owner reopen; actual OS authentication/listener/adoption remain separate. [Evidence](../validation/MV-8-host-workspace-service.md).

### ADR-MEM-77 — Explicit Windows Host process and pipe prototype (MV-8.1h)

[Decision](077-windows-host-transport.md): existing v2 root, explicit live-process enrollment, protected local pipe and bounded versioned native frames, with reciprocal kernel PID checks and continuous lease through lock/reconnect. Synthetic executable evidence is separate from native client/Runtime adoption and multi-client/MCP acceptance. [Evidence](../validation/MV-8-windows-host-transport.md).

### ADR-MEM-78 — Native client reconnect boundary (MV-8.1i)

[Decision](078-native-client-reconnect-boundary.md): native-only queue tickets bind work to one connection generation, per-window scope is checked independently of page packets, and uncertain replies disconnect without retry. Explicit lock/unlock/reconnect remain separate from ordinary Call frames. [Evidence](../validation/MV-8-native-client-reconnect.md).

### ADR-MEM-79 — Opt-in reference shell native Host (MV-8.1j)

[Decision](079-reference-shell-native-host.md): explicit native arguments select a hidden, enrolled child Host and a connection-ticket-aware reference backend. Native tray reconnect/unlock, unavailable picker/autostart gates and visible uncertain-call errors keep the default embedded mode separate. [Evidence](../validation/MV-8-reference-shell-host.md).

### ADR-MEM-80 — Restricted MCP stdio boundary (MV-8.2a)

[Decision](080-restricted-mcp-stdio-boundary.md): owner-authorized local synthetic MCP 2025-11-25 adapter; existing six-tool Memory IPC envelopes, native Agent scope ceilings, revocation, bounded stdio and original-packet retention after uncertain outcomes. No owner Workspace bridge, actual Host/domain binding, real client, Provider, private data or cloud activation.

### ADR-MEM-81 — Host-scoped Agent reads (MV-8.2b)

[Decision](081-host-scoped-agent-reads.md): in-process MCP read binding over the existing Host Vault, separate expiring Agent connections, current persisted policy checks, deletion/provenance barriers and fail-closed content-free auditing. No owner Workspace bridge or external Agent transport enrollment.

### ADR-MEM-82 — Host Agent proposals and keyed receipts (MV-8.2c)

[Decision](082-host-agent-proposals.md): authorize actual Agent proposal dependencies over the same Host writer, retain native principal and canonical-operation/key receipts, keep candidates pending and refuse unreceipted global duplicates without disclosing their IDs. Unknown publication/audit results close the session without replay.

### ADR-MEM-83 — Agent local current-memory context (MV-8.2d)

[Decision](083-host-agent-local-context.md): bind current, session-free context_get to the existing compiler with native McpClient provenance, current policy filtering, fixed offline destination and requestId receipt recovery. Unsupported history/session/destination choices are refused, not ignored.

## Relation to Enouia Runtime's register


Runtime [ADR-025](https://github.com/Morii9961/enouia-runtime/blob/main/docs/adr/025-enouia-memory-integration.md) is the counterpart of ADR-MEM-45: Runtime records this repository as the Memory domain authority and its pinned-revision dependency. Runtime's own register is authoritative for the status of Runtime ADRs. As amended by ADR-025: Runtime ADR-001 (local canonical ownership), 004 (shell/domain separation), 010 (encryption), 011 (embedded Core), 016 (two-track delivery) and 017 (health) are amended; 002 (Memory format), 006 (schema and candidate lifecycle), 007 (Mock Provider first), 008 (replica) and 009 (bridge and gateway) are superseded by this register; 003 (disposable index) is superseded for its Memory part and keeps its Activity clause; 012 (independent Activity) is unchanged. Runtime's own Core designs, Runtime ADR-020 to 024 (Vault generations, index and retrieval, invocation ledger, command receipts and jobs, operator export), are superseded for the Memory domain; their numbers are unrelated to ADR-MEM-20 to 24. Always write "Runtime ADR-0NN" or "ADR-MEM-NN".
