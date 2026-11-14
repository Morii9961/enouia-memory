# Synthetic initialization

Native, opt-in initialization for a **new synthetic Owner segmented root**. There is
no directory discovery, real-data CLI, model adapter or Runtime activation.
The ADR-MEM-93 fields were confirmed by the owner before this implementation.
See [ADR104](../../docs/adr/104-synthetic-initialization-intake.md) and the
[measured report](../../docs/validation/Memory-init-pipeline.md).

From the repository root, using the pinned Rust toolchain:

```powershell
$env:CARGO_NET_OFFLINE = 'true'
cargo run -p enouia-memory-init --example synthetic_init --locked
```

The example uses S01–S32 and additional built-in synthetic display evidence
(37 statements, a nonempty theme, month intervals and same-name entities), creates a unique temporary
root, performs preflight, interrupts and reopens the journal, completes native
formation, builds seven projection groups, deletes a synthetic record and
rebuilds all groups. It prints the root location and retains that root for
inspection. `indexes/initialization-report.json` records measured coverage;
the `before` projection artifact is deliberately stale after deletion.
Saved bundles cannot authorize reads: consumers must use native readers and
compare their snapshot, policy, deletion and withdrawal bindings.

## Native API flow

1. Pass explicitly selected in-memory `Material` values to `parse::prepare`.
   Use `prepare_with_document_metadata` for selected document author, modification
   time and language; those values become part of the input watermark. Aliases
   accept no filesystem paths. This library does not open source files.
2. `Plan::build` takes scripted `Statement` analysis, a supplied bounded policy,
   an explicit source-capture time and `Backfill` or `Live`. It resolves exact
   scalar quotes, preserves source clocks/branches/metadata, constructs record
   fields and returns preflight without I/O. Event time never falls back to a
   source timestamp. Unknown language is `und`.
3. Create a synthetic initializer only on a new empty
   `automation-segmented-owner-v1` root with explicit `owner_subject_id`, or
   reopen its saved journal. Genesis witnesses the descriptor digest; the
   `sync-state/initialization-owner-1.json` journal binds the same digest.
   Old/unbound descriptors and a different attributed owner are refused.
   `enqueue` archives immutable selected bytes and saves the exact plan,
   analysis and IDs. Repeating a watermark/mode with different analysis refuses.
   Different source sets need distinct supplied policy identities; the
   initializer cannot silently change an existing batch's policy.
4. Call `step` until it returns false. Each dispatch prefers live work. Source
   capture is followed by the same native prepare/evaluate/witnessed-apply route
   used for ordinary formation. Persisted uncertain application performs original
   receipt lookup only; missing receipts remain unknown. It never regenerates
   or resends them. A poisoned instance must be reopened.
5. `ledger` measures received, parsed and committed layers separately; unmeasured
   model quality, platform totals and recall quality remain null. Call
   `projection_bundle` with an explicit IANA timezone and month-aligned date
   bounds for native time/theme/graph/attention/facet/activity reads and literal
   search inspection rows. Existing native catalog filtering provides literal,
   profile-alias and supported entity-alias matching.

All notes, tags, tone, importance, current-state notes and suggested relations
remain revision-bound advisory annotations. No owner decision is generated.
Complete legacy v1 RecordSets require exact original Review text witnesses and
the separate native owner-selected capture boundary. Unreviewed candidates and
derived buckets remain model/agent claims. Original records are never rewritten.

`Format::OwnerProvided` accepts an explicitly selected JSON packet with `content`
and optional `content_lang`, `intake_key`, `original_quote` and
`original_quote_speaker`. The owner's statement is an ordinary user-statement
source formed in the same batch, without an extra review queue. The quote is a
separate source retaining its speaker/evidence class; a quoted assistant is not
the owner. An unbound quoted user identity stays unknown. The parser never
selects this mode from consent-looking JSON or creates UserConfirmation or an
OwnerAction. Real intake must authenticate the native owner's selection; this
pure parser is not that authentication boundary.

## Consuming the synthetic root

The example prints `synthetic_root=<temporary path>`. After the command exits
and its native writer closes, open that exact root through `verify_data_root` and
`SegmentedAutomationVault::open` with its descriptor's host ID. In the example,
the fixture clock is explicitly fixed at `2026-10-24T12:00:00Z`; a test reader
must use that clock to keep the synthetic policy valid. Use an independently
authenticated native reader, not `Initializer::open_synthetic` as Agent
authority. The Owner catalog selection is `owner-qualified-catalog-4`/v4;
process headers use v2. The final root has 36 visible records after one test
deletion; its saved before bundle is stale. Keep the root and its Raw objects
local, and compare current snapshot/barriers through native reads.

Import carriers use `CapturedSource` with typed `attachments`, SourceRecord
attachment refs, original locators/clocks and revision-bound evidence. Native
`capture_import` verifies Raw closure and refuses owner confirmation; legacy
confirmation has a separate native-owner-selected boundary. These are not MCP
or Runtime capture routes. All four import-specific native entries (capture,
legacy capture, Raw archive and receipt lookup) require an Owner descriptor
before authentication, object work, locking or record access; the old
`SegmentedV1` layout cannot opt in through parser/model parameters. The ordinary
runtime-only capture entry refuses typed attachment carriers. Old prototype
roots are not migrated or offered for formal adoption.
Already captured Runtime sources must retain their
original `SourceRevisionRef` and capture key: the initializer does not yet expose
a queue-only entry for those revisions, so do not repackage them as imports.

## Explicit limits

At most 64 parsed sources and 128 statements per plan; each material and the
journal are bounded at 16 MiB, each source text at 16 KiB. Refuse oversized work
rather than truncate it. The projection envelope permits at most 128 original
dependencies. No large-history throughput claim is made.

Supported input shapes: existing ChatGPT JSON/ZIP, Runtime-session/1 and Markdown
adapters, synthetic-observed Claude `uuid/chat_messages` JSON, validated complete
v1 snapshots, JSON derived buckets and UTF-8 text documents. Unsupported bytes
are retained and counted. YAML buckets, binary extraction/OCR, platform-version
discovery, changed-upstream-message reconciliation and real snapshot acquisition
remain unavailable. Exact repeated messages dedupe within platform/account/
conversation identity; changed content under the same message identity refuses
at plan validation rather than silently overwriting history.

The script is **not** a semantic extractor or independent model evaluation.
One integration test exercises a separate native Owner gesture after ordinary
formation; initialization itself generates no Owner action. Runtime six-view previews/screenshots, real data,
egress/default-timezone authorization and Core-8 switching are separate work.

## Fresh ChatGPT selection (ADR140)

`fresh::prepare_chatgpt_selection` accepts explicitly supplied shard bytes and
shard-scoped conversation IDs. Large outer shards are read value by value; only
selected conversation spans become materials. Each is the exact original JSON
value wrapped in two array brackets, still bounded at 16 MiB. No original
conversation is reserialized. Unselected Raw bytes are excluded.

Each message retains the original message metadata under `raw_message` and the
shard hash/length, alias/account, conversation ID/index and byte span under
`fresh_chatgpt_origin`. The same origins and derived hashes bind the preparation
watermark. Keep and back up complete original shards outside the Vault; these
references cannot reconstruct them. No path discovery or selection authority is
provided by parsing.

This entry prepares materials only. It does not invoke analysis, resolve sibling
attachment files, truncate oversized source text, lift formation/journal limits,
create a production root, or configure a long-term reading policy. Fresh semantic
analysis and the production driver must be delivered separately; the legacy
`enouia-memory-reinit` command still consumes legacy candidates only.

## Supplied fresh analysis (ADR-MEM-142)

`Plan::build_analyzed` prepares a caller-supplied `AnalyzedBatch` through the
existing formation pipeline. It keeps distilled content/title, explicit analysis
actor/prompt/optional model identity, and multiple exact supporting source quotes.
It calls no Provider. Distinct exact ranges from the same source are retained
under ADR-MEM-143; unsupported or repeated ranges refuse instead of being dropped.
This is a preparation API,
not a corpus analyzer, packer, production CLI or read authorization. Use the
existing authenticated bootstrap/journal/receipt route for any later execution.

See [decision](../../docs/adr/142-fresh-authored-formation.md) and
[focused synthetic validation](../../docs/validation/Fresh-authored-formation.md).

## Bounded supplied-analysis packing (ADR-MEM-144)

`packing::pack_analyzed` groups an explicitly prepared `AnalyzedBatch` into
64-source/128-statement plans, keeping all quoted support and context with each
statement. It preserves wording, provenance, Raw bytes, locators, attachment
carriers and original branch lineage, and returns original statement positions
for coverage checks. Every returned batch still needs `Plan::build_analyzed`.

An individual statement exceeding 64 sources, oversized source/Raw objects,
missing dependencies and cross-statement revision/derivation links refuse the
whole preparation. Revise semantic units explicitly; the packer never truncates
or drops evidence. Unused objects are omitted, but selected Raw conversation
objects retain their surrounding original context; this entry is not redaction.
Keep the original intake counts separately instead of summing them per plan.

This pure entry does not resolve attachment files, execute the bounded journal,
bootstrap a production root, authorize reads or supply a fresh production CLI.
Production use waits for Owner merge. See [decision](../../docs/adr/144-fresh-analysis-packing.md)
and [focused validation](../../docs/validation/Fresh-analysis-packing.md).
