# ADR-MEM-135 — Re-initialize the legacy library into a new Owner Vault

2026-11-10. Proposed by Morii's migration decision. The data the ChatGPT
connector should serve is not in the empty Owner root created on 2026-10-30.
It is in an older v1 library:

- sources and attachments from one ChatGPT export import;
- candidates extracted by a model during the seed stage, never reviewed;
- the original export archive.

The library has no approved memories. Paths stay in local configuration only.

## Why not upgrade the old root

A `SegmentedOwnerV1` root binds the Owner subject in its genesis commit
through `descriptor_hash` (ADR-MEM-103). Every later commit chains from that
genesis. The v1 root has no such binding. Adding it after the fact would mean
rewriting the genesis and every commit after it. The design therefore never
upgrades or renames a root (INITIALIZED_VAULT). Re-initialization creates a
new root and forms its contents from the old library as input. It is the
supported path, not a workaround.

## Decision

1. **Owner decisions.** Morii decided the following:
   - Every candidate becomes a formal memory directly. They are
     initialization output: there is no review queue, no per-item
     confirmation and no model call.
   - Evidence stays what it was, a legacy model claim. It is never recast as
     Owner confirmation.
   - The timezone is Asia/Shanghai.
   - The target is a Linux VPS.
   - After the new root passes acceptance and has a verified independent
     backup, the old library may be deleted. Morii confirms once more before
     deletion.
2. **Read-only export.** A new export step opens a *working copy* of the v1
   Vault, never the original, pins its current commit and writes:
   - the original ChatGPT export objects, byte for byte;
   - a `RecordSet` snapshot envelope (`Format::LegacySnapshot`) of the pinned
     records, excluding purged content and records behind a tombstone.

   Materials are split to stay under the 16 MiB material limit. The original
   library is only read.
3. **Production initializer.** Today the initializer only offers
   `create_synthetic` and `open_synthetic`. Add a production entry with these
   properties:
   - It creates the root through the native Owner bootstrap (ADR-MEM-110 and
     134) and runs as the authenticated Owner.
   - It binds the selected source scope, Asia/Shanghai and a "no provider"
     analysis mode.
   - It builds statements deterministically from the legacy candidates, one
     statement per candidate. Each statement quotes the candidate text
     exactly. Where the candidate's evidence maps to a chat message from the
     same export, it adds a context reference to that message. No model is
     called.
   - It splits plans under the existing capacity limits (64 messages and
     128 statements). A candidate and the chat messages it cites go in the
     same plan.
   - It keeps the existing journal, receipt and unknown-outcome behaviour:
     only the original operation may be looked up, and nothing is re-sent
     under a new key.
4. **Verification before use.** The run reports:
   - expected and actual counts of sources, attachments, candidates and
     memories, and the exclusions from legacy deletions;
   - a sample of source jumps back to the export text;
   - the deletion barriers.

   The new root is backed up and the backup restored in a scratch location
   before the old library is offered for deletion.
5. **Upload.** The verified root, with its owner-data home, moves to the VPS
   under the service account (ADR-MEM-133) and is opened with its own host id.
   The policy and source scope exposed to ChatGPT is still Morii's decision.

## Progress

- 2026-11-10: step 2 landed as `enouia-memory legacy-snapshot <vault> <empty-dir>`.
  It writes `snapshot.json` (every record revision, validated as a
  `RecordSet`), `objects/<sha256>` (import inputs and stored attachments, byte
  for byte) and `manifest.json` (counts, object roles, purged items skipped).
  It refuses a non-empty destination and does not write to the Vault. The
  evidence is synthetic; see the `legacy_snapshot_*` CLI test.
- 2026-11-10: step 3 landed.
  - **Production entry.** `Initializer::adopt_bootstrapped` takes over a root
    that the Owner bootstrap has just created. It refuses a root that already
    holds sources, memories or a journal.
  - **Batching.** `legacy::legacy_batches` matches old sources to re-parsed
    export messages, by message id or by content hash for documents. It turns
    every live candidate into one self-report statement and packs batches
    within plan capacity.
  - **Driver.** `enouia-memory-reinit plan|run`.

  Details that differ from the plan above:
  - **Quote.** The quote is the cited owner message. A message longer than the
    500-character quote limit is excerpted to the sentence sharing the most
    character pairs with the candidate's wording, widened by its neighbours.
  - **Title.** The title is the candidate's wording when the quote contains
    it. Otherwise it is the first line of the quote: titles may not hold
    control characters.
  - **Kind.** Project-state candidates form facts, because a project state
    needs a stage the candidate never stated.
  - **Note.** The candidate's wording and id stay in the formation note.
  - **Other citations.** A candidate's other cited messages become context
    dependencies only when they share its batch. The rest are counted as
    dropped. Batching never merges conversations for them, because that
    chained most of the library into one oversized batch.
  - **Long conversations.** The old library kept only the owner's turns, so
    each turn is its own branch (a plan allows 32 per conversation). A
    conversation therefore contributes at most one 32-turn chunk per batch.

  Evidence: [validation](../validation/Legacy-reinitialization.md).

## Not in this slice

The code. This ADR records the decision and the route. The export, the
production initializer and the verification run land in later slices, each
with synthetic evidence first and the real run only on Morii's go-ahead.

## Consequences

The migrated memories carry legacy-model-claim evidence and point back to the
original export text. Any later re-check with a real provider is a separate
step (MV-7). The empty Owner root from 2026-10-30 is not used for this
migration.
