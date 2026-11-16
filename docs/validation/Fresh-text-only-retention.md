# Explicit text-only fresh retention — synthetic validation

2026-11-16. [ADR148](../adr/148-fresh-text-only-retention.md), based on merged
main `a543c1eb81e5d7150ebe7820592eea35a9042697` (PR111).

## Behavior and scope

An explicit manifest `attachment_retention: metadata_only` retains all parsed
attachment slots and metadata without opening/copying originals. Original-file
bindings conflict with this mode. Preflight distinguishes intentional non-selection
from unavailable originals, and native attachments retain the descriptive
`memory-init.attachment-retention` extension after reopen. Existing availability
remains a property of the received JSON carrier, not a search of external files.
Omitting the field preserves previous `selected_originals` behavior.

Textual Raw still contains complete selected conversations, including surrounding
unselected messages, and the exact original authored analysis file. Quote checks,
speaker/branch, contexts, temporal/subject/sensitivity metadata and visual-analysis
provenance are unchanged. No attachment content assertion or new owner confirmation
is created. No capacity limit, native admission, external deletion or read grant is
changed. Retention changes conflict with an enrolled run.

## Checks

- `cargo test -p enouia-memory-init --locked`: **64 passed, 0 failed, 1 ignored**
  across 10 targets including empty unit/doc targets. `fresh_driver` contributes
  9 passed and the explicitly gated trusted CLI test.
- That compiled `fresh_driver` test binary was then invoked directly with
  `trusted_cli_bootstrap_forms_reopens_queries_and_reuses_original_run --ignored
  --exact --nocapture`: **1 passed**. Its explicit durable synthetic parent creates
  a unique protected test home, retained with its markers. The production CLI
  forms two text-view memories without attachment payloads, repeats the same run,
  closes/reopens, resolves exact source quotes and explicitly recovers. Changed
  analysis conflicts; no Owner action or read grant is synthesized.
- The 65-source/65-memory case spans two bounded plans, reopens during execution,
  checks every saved journal against the whole-lifecycle bound, pages all records,
  queries a precise Unicode excerpt, retains attachment non-selection metadata,
  and verifies repeat-run identity plus retention/analysis conflicts.
- Dry-run reports zero writes and distinguishes all three metadata-only slots
  from unavailable originals. Conflicting original bindings, unbound slots and
  unknown modes refuse. Default selection, present-original closure and the
  unchanged context/authored-artifact tests pass. The existing whole-journal
  overflow and original-binding uncertainty-barrier tests also pass.
- `cargo clippy -p enouia-memory-init --all-targets --locked -- -D warnings`,
  `cargo fmt --all -- --check`, `git diff --check` and public-content review pass.

Checks are scoped to the initializer change. Contract/schema/native admission and
tracked Runtime surface code are unchanged; no full-workspace, schema or surface
rerun is claimed. All test inputs are synthetic. No real input has been processed
with this new mode, and no real Vault has been created or written by this increment.
The earlier incomplete real-data diagnostic belonged to the superseded original-
attachment selection; it supplies no full-plan or lifecycle-capacity acceptance.
Actual complete text-only corpus capacity remains pending after PR merge.

No Runtime checkout, deployment, data deletion or project status snapshot is
changed. A reviewable PR must merge before real use. Native-library acceptance
and independently verified backup/restore remain separate delivery gates.
