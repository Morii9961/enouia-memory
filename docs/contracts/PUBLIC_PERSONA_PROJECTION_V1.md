# Public persona projection v1 (`public-persona-projection-1`)

2026-11-08, [ADR-MEM-128](../adr/128-public-persona-projection-export.md).
Schema: [`contracts/persona/public-persona-projection-1.schema.json`](../../contracts/persona/public-persona-projection-1.schema.json).
Rust: `enouia_memory_contract::persona` (`parse_document`, `extract_block`).
Synthetic samples: [`tests/fixtures/persona/`](../../tests/fixtures/persona/).

A generic, closed document that an owner exports for a public audience. It is
written for a consumer that must never receive non-public content, never
connects to Memory, and whose readers may extract everything it contains and
send it to a model provider. Every object rejects unknown fields; an unknown
`contract`, `audience` or field is a refusal, never a guess.

## 1. What may enter

Only two kinds of content:

1. **Identity part.** The JSON body of exactly one fenced block in one exact,
   current, owner-approved Identity revision:

   ````markdown
   ```enouia-public-persona-1
   { "display_name": "…", "persona": { … }, "style": { … }, "examples": [ … ],
     "interests": [ … ], "boundaries": { … } }
   ```
   ````

   The block's words are approved through the ordinary Identity review, because
   they are part of the Markdown. Nothing outside the block is read. Zero or
   several blocks, an unclosed block, non-JSON, unknown fields, or a value
   outside the bounds below refuse the preview.

2. **Public knowledge.** A Memory record that the owner listed for this
   audience **and** whose `sensitivity` is `public` **and** that is active,
   intact, not tombstoned and inside its valid time. Either condition alone is
   not enough. A listed record that fails any condition is excluded and counted;
   its text never appears.

## 2. Document

The file is the canonical bytes of this object: keys sorted, two-space
indentation, UTF-8, LF line endings, one trailing LF (`json::canonical_bytes`;
Python: `json.dumps(v, sort_keys=True, indent=2, ensure_ascii=False) + "\n"`).

| Field | Meaning |
| --- | --- |
| `contract` | `"public-persona-projection-1"` |
| `projection` | Everything the consumer may use (below). |
| `projection_sha256` | SHA-256 over the canonical bytes of `projection`. Unchanged content keeps this digest and the version. |
| `version` | Positive integer. Same content → same version; changed content → next integer. A number never names two different contents. |
| `exported_at` | UTC timestamp `YYYY-MM-DDTHH:MM:SS.mmmZ`. Outside the digest. |
| `memory_version` | Exporting software, e.g. `enouia-memory-workspace/0.1.0`. Outside the digest. |

`projection`:

| Field | Meaning |
| --- | --- |
| `projection_id` | `ppj_` + 32 hex. Stable per Vault and audience (derived, not stored). |
| `audience` | `public_group` (only value in v1). |
| `source` | `{projection_id, identity_id, identity_revision, contract}` for tracing. Not a signature. |
| `display_name` | Public name, 1–64 characters. |
| `persona` | Seven texts, each 1–4000 characters: `background`, `personality`, `voice`, `motivation`, `values`, `rules`, `limits`. |
| `style` | `rules`, `anti_examples`: up to 32 unique strings of ≤500 characters. |
| `examples` | 3–8 dialogues; each `turns` has 2–16 `{speaker, text}` (speaker ≤64, text ≤2000). |
| `interests` | Up to 32 unique strings of ≤200 characters. |
| `public_knowledge` | Up to 64 items sorted by `item_id`: `{item_id, text, source: {memory_id, memory_revision}, tags}`. `item_id` (`pkn_` + 32 hex) is stable across revisions of the same record; `text` is the record content (≤20000); `tags` are the record's labels (may be empty). |
| `boundaries` | `refuse` (≤64 × 500), `ai_disclosure` (1–1000), `avoid_topics` (≤64 × 200), `forbidden_words` (≤128 × 64). |

Every text is non-empty, not whitespace-only, and has no control character
other than LF. Rust also refuses credential-shaped and local-path-shaped text
(the schema cannot express these, nor the digest and item order).

Approval records and per-group settings are not part of this contract; the
consumer keeps them.

## 3. Owner operations (Workspace IPC v1)

All three run on the trusted owner surface and are additive.

**`persona_options {audience}`** (read, null key) → `persona_options`:
`projectionId`, Identity records (`identityId`, `revision`, `slug`, `title`,
`sensitivity`, `publicBlock: valid|missing|invalid`), eligible public records
(`memoryId`, `revision`, `title`, `type`, `tags`, `namesPeople`; at most 500,
with `publicMemoriesTotal` and `truncated`), and `lastExport` (version, digest,
Identity revision, the owner's last selection, file name and SHA-256) or null.
Non-public records are not listed.

**`persona_preview {audience, identityId, identityRevision, memoryIds}`** (read,
null key) → `persona_preview`: `projection` (exactly the object that will be
written), `projectionSha256`, `version`, `previewDigest`, `memoryVersion`,
`contract`, and:

- `diff` against the last successful export: `baselineVersion`, changed
  Identity fields by JSON pointer, and per-item `added|changed|removed` with the
  source reference. Removed items show only their ID and source, never text.
- `exclusions` (counts only): non-blank Identity lines outside the block, other
  Identity records, unselected non-public records by sensitivity, unselected
  public records, selected-but-not-public records, selected-but-unavailable
  records (archived, superseded, forgotten, deleted, outside valid time, broken
  provenance or unknown).
- `warnings`: `relationship_identity` when the Identity slug is `relationship`;
  `names_people` with the item IDs whose records name subjects or participants.
  Memory cannot judge meaning; these are reminders for the owner.

The Identity revision must be the current head of a non-tombstoned record
(`persona.identity_stale`, `persona.identity_missing`). At most 64 unique
`memoryIds`. A block that breaks §1–§2 refuses with `persona.identity_block`
plus the specific rule; an eligible record whose text breaks §2 (for example a
local path) refuses with `persona.projection_invalid` plus the rule, so the
owner fixes or unchecks it rather than having it silently dropped.

**`persona_export {…selection, previewDigest, destinationToken}`** (write,
idempotency key required) → `persona_exported` receipt: `audience`,
`projectionId`, `version`, `projectionSha256`, `fileName`
(`public-persona-<audience>-v<version>.json`), `fileSha256` (of the exact file
bytes), `bytes`, `exportedAt`, `memoryVersion`, `replayed`.

- The preview is recomputed; a different digest refuses with
  `revision_conflict` / `persona.preview_changed`.
- `destinationToken` comes from the native picker kind `persona_destination`.
  Refused folders (`persona.destination_rejected` plus the reason): inside a Git
  repository or worktree (`root.inside_repository`), inside the open Vault or
  any Vault root (`persona.destination_in_vault`), network, cloud-sync, system,
  removable, reparse-point or non-canonical paths (the `root.*` rules).
- An existing file is never replaced (`persona.destination_exists`).
- The version is reserved in the ledger before the file is written.
- The same key with the same request replays the receipt (`replayed: true`)
  without touching any folder. The same key with a different request is
  `idempotency_conflict`. If an earlier attempt reserved the version but its
  outcome is unknown, only the owner's resend of that same request completes it:
  the reserved bytes are written, or an identical existing file is accepted.
  Memory never retries by itself, never schedules or syncs, and never uses the
  network.

## 4. Consumer checks

1. `parse_document` semantics: closed shape, `contract` equals this name, all
   bounds, item order, digest recomputes from the bytes.
2. Compare `fileSha256` from the receipt with the file the owner copied.
3. Refuse a known `projection_id` + `version` whose `projection_sha256`
   differs from what was seen before.

The samples in `tests/fixtures/persona/` are real Core output from synthetic
data (`persona_contract_samples`, ignored test). `projection-sample.json` is a
version-2 document; `preview.json` shows a diff with added, changed and removed
items; `manifest.json` lists negative cases with the expected rule.

## 5. Open (待 Morii 决定; the conservative option is implemented)

1. **Marking the public part.** Implemented: one fenced block inside an Identity
   revision. Alternative: a dedicated Identity slug for public personas.
2. **Older Identity revisions.** Implemented: only the current head may be
   exported. Alternative: allow any approved revision.
3. **Ledger loss.** The version ledger is local state outside the commit chain
   and is not restored from a backup. After a loss the version restarts at 1
   under the same `projection_id`, which the consumer's rule refuses (fails
   closed). Alternative: keep the ledger as Vault records.
4. **Destination policy.** Implemented: the Vault-root policy, so cloud-sync
   folders and removable drives are refused too. Alternative: allow those for a
   file meant to be carried to another host.
5. **Tags.** Implemented: the record's own tags are exported. Alternative: omit them.
