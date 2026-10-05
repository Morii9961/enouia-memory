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

## ADR-MEM-19 … 45 — implementation decisions (MV-0 / MV-0R / MV-1 … MV-6, Runtime integration)

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

### ADR-MEM-45 — Runtime hosts Memory's local client (Adopted, Runtime integration)

Refines ADR-MEM-19 and amends the hosting part of ADR-MEM-44. The handoff, host duties, change routing, and compatibility log are in [integration/RUNTIME.md](../integration/RUNTIME.md).

- **Product client.** Enouia Runtime's Windows client (`Morii9961/enouia-runtime`, `apps/desktop`) is the product client for Memory's local part. It embeds `enouia-memory-workspace` at a pinned Git revision (a full commit SHA, never a branch; no path dependency or `[patch]` in committed manifests) and adds its own adapter: native pickers that return tokens, window scoping, and lifecycle. Memory still never depends on Runtime and builds and tests alone.
- **One seam.** A host calls only `Workspace` (`new`, `call`, `register_pick`, `open_root`, `set_companion`, `shutdown`) with workspace IPC v1 envelopes. It never calls the domain crates directly. When MV-8 replaces the embedded Core with the single Memory Host, the envelope stays and only the host transport changes; that switch is a logged surface change with a Runtime migration.
- **Shared scope rule.** `HostSurface` (`Workspace`, `QuickSearch`) in `enouia-memory-contract::workspace` is the one caller scope. Hosts derive it from their native window identity and check it before forwarding. The reference shell maps `main`/`overlay` onto it; Runtime maps its own windows. Page-supplied fields never widen it.
- **Reference shell.** `apps/workspace` stays here as the reference shell and acceptance harness: MV-6 evidence, installer tooling, and later MV-8.1 reconnect tests. It is not the product client. New product UI for the local part lands in Runtime first; changes to the reference shell or frontend are logged for Runtime like any other surface change.
- **Integration surface.** `docs/integration/runtime-surface.json` records every file Runtime depends on (wire contract, Core crate, build closure, reference shell and frontend) with its digest. `crates/enouia-memory-contract/tests/runtime_surface.rs` fails until a change is regenerated with `tools/integration/runtime_surface.py` and its aggregate is logged in `integration/RUNTIME.md`. The test forces an acknowledgment in this repository; it cannot prove Runtime adopted the change.
- **Trusted surface.** Runtime-hosted reviews are stamped `trusted_windows_app`. That holds only while the host keeps the W04 guarantees: strict CSP, no `fs`/`shell`/`http`/dialog plugins, plain-text rendering, no paths from the page, and no logs of request or response bodies.
- **One Core per Vault.** The reference shell, Runtime, and the CLI can each open the same Vault. Commits serialize through the writer lock, but two embedded Cores also hold the index file. Run one embedded Core per Vault at a time until MV-8's Host owns the lock.
- **Data root.** Hosts open only an explicitly chosen root (ADR-MEM-37, 44). The `%LOCALAPPDATA%\EnouiaMemory` default named in ADR-MEM-19 is not used. A remembered or default root in any host needs a Memory ADR.
- **Cloud stays here.** MV-7 (real Provider), MV-8 (Host, MCP), MV-9 (gateway, queue), MV-10 (replicas), MV-11, `contracts/ipc/memory-v1.schema.json` (the agent surface), `contracts/provider/capabilities-v1.schema.json`, backup, and their code and ADRs remain in this repository. Runtime implements no Provider calls, MCP, gateway, or replica, and never carries Memory sync over its Activity delivery path.
- **Activity.** Activity data never enters workspace IPC. A host replaces the Core's fixed Activity status row (`component: "activity"`, `mode: "independent_not_managed"`) with its own Activity health.

## Relation to Enouia Runtime's register

Runtime [ADR-025](https://github.com/Morii9961/enouia-runtime/blob/main/docs/adr/025-enouia-memory-integration.md) is the counterpart of ADR-MEM-45: Runtime records this repository as the Memory domain authority and its pinned-revision dependency. Runtime's own register is authoritative for the status of Runtime ADRs. As amended by ADR-025: Runtime ADR-001 (local canonical ownership), 004 (shell/domain separation), 010 (encryption), 011 (embedded Core), 016 (two-track delivery) and 017 (health) are amended; 002 (Memory format), 006 (schema and candidate lifecycle), 007 (Mock Provider first), 008 (replica) and 009 (bridge and gateway) are superseded by this register; 003 (disposable index) is superseded for its Memory part and keeps its Activity clause; 012 (independent Activity) is unchanged. Runtime's own Core designs, Runtime ADR-020 to 024 (Vault generations, index and retrieval, invocation ledger, command receipts and jobs, operator export), are superseded for the Memory domain; their numbers are unrelated to ADR-MEM-20 to 24. Always write "Runtime ADR-0NN" or "ADR-MEM-NN".
