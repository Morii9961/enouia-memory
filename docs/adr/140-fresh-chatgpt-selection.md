# ADR-MEM-140 — Select original ChatGPT conversations from large shards

2026-11-11. Morii replaced the legacy-candidate migration with fresh analysis of
explicitly selected raw conversations. The existing legacy CLI is not that
analyzer. Some supplied shards exceed the initializer's 16 MiB material limit.

## Decision

Add a pure `fresh::prepare_chatgpt_selection` native library entry. The caller
supplies shard bytes, opaque aliases/accounts and exact conversation IDs scoped
to their shard aliases. It discovers no paths and grants no selection authority.
Duplicate, ambiguous or missing selections refuse the complete preparation.

Read each top-level array value with the existing pinned serde_json stream
parser. Its successful `byte_offset` identifies the exact conversation byte span.
Wrap only selected spans in an array for the existing ChatGPT adapter. Preserve
spacing, escapes, field order and unknown fields inside each selected value;
this is a derived two-bracket envelope, not the original shard. Each envelope
must still fit the existing 16 MiB limit. No constant, dependency or feature changes.

Bind the original shard hash/length, alias/account, conversation ID, array index,
byte span and derived-object hash into the selection watermark and each parsed
message's metadata. Retain original message metadata separately, unchanged.
The parser keeps roles, evidence, times, parent links and unknown values under
its existing rules. Selection does not create confirmation, analysis or memories.

Full original shards remain immutable in the owner's separately retained input
archive. Their hashes and spans establish provenance; they do not reconstruct
that archive from the Vault or claim independent backup. Unselected conversation
bytes never become prepared Raw objects. Backup of external originals remains a
separate requirement.

The stream API is documented by [serde_json](https://docs.rs/serde_json/latest/serde_json/struct.StreamDeserializer.html#method.byte_offset)
and was checked against the installed pinned 1.0.151 source. Use offsets only
after successful object deserialization, with explicit array delimiters.

## Verification and remaining work

Use synthetic large shards, Unicode/escapes/whitespace, exact span/locator checks,
selection refusals and watermark changes. No real input is passed through new
code before owner merge approval. No Runtime surface, schema, model/Provider,
real Vault, deployment, deletion or new MV stage is part of this slice.

This is the selected-material intake prerequisite. Attachment byte lookup,
untruncated oversized message handling, semantic Statement analysis/provenance,
bounded formation batches/journal capacity, a production fresh-input driver and
long-term read policy remain separate work. Do not call this a completed fresh
initialization or replace those steps with the old 185-candidate snapshot.

Validation: [synthetic selected-material checks](../validation/Fresh-chatgpt-selection.md).
