# Fresh ChatGPT selected-material intake (ADR-MEM-140)

2026-11-11. Based on merged main `45de3c0`. Synthetic data only.

## Changed behavior

`fresh::prepare_chatgpt_selection` prepares exact conversation JSON spans selected
by shard alias and conversation ID. A shard can exceed the ordinary material
limit while each selected derived envelope remains bounded at 16 MiB. Source
metadata and selection watermarks record original shard hashes and byte spans.
The complete original shard stays separately retained; it is not reconstructed
or silently imported with unselected conversations.

## Checks actually run

- `cargo test -p enouia-memory-init --test fresh_chatgpt --locked`: 4 passed.
  A synthetic outer shard larger than 16 MiB retains Unicode, escapes, whitespace,
  field order and unknown fields in the selected original span. Original input
  hash is unchanged; prepared Raw excludes the unselected conversation. Quotes
  resolve, speaker/evidence classes and parent/model metadata remain intact.
  Changing the outer shard changes the watermark even with identical selected text.
  Further cases cover multiple shards, missing/duplicate/ambiguous selection,
  malformed array boundaries, oversized selected conversations and unsupported
  selected shapes. None performs filesystem or Vault I/O.
- `cargo clippy -p enouia-memory-init --lib --test fresh_chatgpt --locked -- -D warnings`:
  passed. Cargo reported the existing incremental-cache hard-link fallback to
  copying in dependencies; this was not a source-code lint failure.
- Targeted `rustfmt --check` on the three affected Rust files and `git diff --check`:
  passed.

No new dependency, feature or schema. Runtime surface files are unchanged, so the
surface checker was not run. No full workspace suite, Linux run, real-input
execution, semantic analysis, Provider, Owner initialization, deployment, deletion
or manual project status refresh was performed by this slice.

## Remaining capability gates

This pure library entry is not the fresh production initializer. Attachment byte
resolution, oversized source handling, explicit semantic analysis/provenance,
bounded formation/journal execution and the fresh-input CLI remain to be delivered.
Long-term reading scope, independent encrypted backup acceptance and ChatGPT/VPS
acceptance retain their own owner decisions and evidence. The old legacy-candidate
CLI is not used as a substitute.

## Mainline refresh (2026-11-12)

Refreshed against merged connector-policy mainline `e667fb4`. The only merge
conflict was the ADR register; both ADR140 and ADR141 are retained in order.
The same four `fresh_chatgpt` synthetic tests passed on this refreshed tree.
No real input or Vault was used, and no additional acceptance is implied.
