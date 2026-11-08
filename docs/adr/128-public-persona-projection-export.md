# ADR-MEM-128: Owner-exported public persona projection

2026-11-08. Adopted for the local Workspace contract (first slice); Runtime
adoption is deferred to Runtime's own authorized change.

## Context

A downstream consumer (Enouia WeChat) lets Enouia take part in group chats
with a public persona. It currently uses a hand-maintained persona pack and
wants Memory to produce it. Three constraints are fixed by that consumer and
by Morii's decisions D16a (owner export, not an Agent exporter) and D16b
(public sensitivity **and** an explicit per-audience selection):

- the bot process never receives non-public content;
- the bot never connects to Memory;
- everything exported can be extracted by chat participants and is sent to a
  model provider.

Identity revisions are owner-approved Markdown (ADR-MEM-40/41). Memory
records carry an independent `sensitivity` (ADR-MEM-06). Nothing in Memory
yet marks part of an Identity revision as public, and no owner operation
writes a file for another program to consume.

## Decision

1. **Contract.** Memory owns `public-persona-projection-1`
   ([contract](../contracts/PUBLIC_PERSONA_PROJECTION_V1.md),
   [schema](../../contracts/persona/public-persona-projection-1.schema.json)).
   It is generic: `audience` names the use; v1 defines only `public_group`.
   Every object is closed. No consumer-specific field is defined; approval
   records and per-group settings stay with the consumer.

2. **Public Identity part.** The owner marks the public part of an Identity
   revision by writing exactly one fenced block with the info string
   `enouia-public-persona-1` whose body is a closed JSON object
   (`identity_block`). Because the block is part of the Markdown, the owner
   approves its exact words through the existing Identity review. Text outside
   the block is never read into the projection; the preview counts its
   non-blank lines without showing them. The selected revision must be the
   current head of that Identity record and not tombstoned; an older or
   deleted revision is refused, so a later approved edit cannot be bypassed.

3. **Public knowledge.** A Memory record enters only when it is (a) listed by
   the owner in the request for this audience, (b) `sensitivity: public`,
   (c) active, not tombstoned and currently valid. Anything else is excluded
   and counted by reason. Selected-but-ineligible records are excluded, not
   refused, so withdrawal (archive, supersession, forgetting, deletion,
   reclassification to non-public) shows up as a removal in the diff.
   Each item carries a projection-scoped stable ID, the record's `content`,
   the exact `{memory_id, memory_revision}` and the record's tags.

4. **Operations.** Three owner commands on Workspace IPC v1, all on the
   trusted owner surface:
   - `persona_options` (read): Identity records with whether they carry a
     valid block, eligible public records, and the last export summary.
     Titles of non-public records are not listed.
   - `persona_preview` (read): the full projection, its SHA-256, the version
     it would receive, a per-item diff against the last successful export,
     exclusion counts, reminders, and a `previewDigest`.
   - `persona_export` (write, idempotency key required): recomputes the
     preview and writes only when `previewDigest` still matches. It writes a
     new file named by Memory into an owner-picked folder (native picker
     token `persona_destination`), never overwrites, and refuses folders
     inside a Git repository or worktree, inside the open Vault or any Vault
     root, on network/cloud-sync/system locations, or behind reparse points.
     It returns a receipt with the file's SHA-256.

5. **Version and digest.** `projection_id` is derived from the Vault ID and
   audience, so it is stable without state. `projection_sha256` is SHA-256
   over the canonical bytes of the `projection` object (sorted keys, two-space
   indentation, UTF-8, LF, one trailing LF, as `json::canonical_bytes`).
   A ledger in the Vault's `sync-state/public-persona-1.json` records the last
   issued `{version, projection_sha256}` and the last successful export.
   Same content keeps the same version; different content takes the next
   integer. A version is reserved in the ledger before the file is written, so
   a number is never reused for different content even after a crash.

6. **Unknown outcomes.** A completed key replays its receipt without touching
   the destination. A reserved-but-unconfirmed key is completed only by the
   owner resending the same request: Memory re-checks the digest, writes the
   reserved bytes if the file is absent, accepts an identical existing file and
   refuses a different one. Memory never retries, schedules, syncs, notifies or
   uses the network.

7. **Not done.** No Agent identity or grant for the bot, no cloud read, no
   automatic or scheduled export, no write-back of chat content, no Runtime
   change. The reference shell gets a minimal acceptance entry only.

## Consequences

- Runtime's product UI must add the selection, preview, the two consumer
  acknowledgements, the folder picker kind and the receipt (logged in
  [RUNTIME.md](../integration/RUNTIME.md)).
- The ledger is local state outside the commit chain: a backup restore does
  not restore it. After a lost ledger the version restarts at 1 under the same
  `projection_id`; the consumer's "same version, different content" rule makes
  that fail closed. Owner decision recorded as open (see contract §Open).
- A synthetic sample document ships with the contract for downstream tests.

Evidence: [public persona export v1](../validation/Public-persona-export-v1.md).
