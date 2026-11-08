# Public persona projection export v1 (ADR-MEM-128)

2026-11-08. Base: `main` b9faf54. Synthetic Identity text and synthetic records
in temporary roots only; no private Vault, cloud, network or Runtime checkout
was opened.

## What was built

- `enouia_memory_contract::persona`: closed `Document`, `Projection`,
  `IdentityBlock`, boundary validation, `extract_block`, `parse_document`,
  derived `projection_id` / `item_id`, canonical `projection_sha256`.
- Schema `contracts/persona/public-persona-projection-1.schema.json`
  (document, Identity block, options, preview, receipt).
- Workspace IPC v1: `persona_options`, `persona_preview`, `persona_export`
  (61 commands), picker kind `persona_destination`, ledger
  `sync-state/public-persona-1.json`.
- Reference shell: page “公开人设导出” (selection, exact projection text, diff,
  exclusion counts, two acknowledgements, folder picker, receipt).
- Synthetic sample corpus `tests/fixtures/persona/` produced by the real Core
  (ignored test `persona_contract_samples`, deterministic clock and IDs).

## Requirement coverage (synthetic)

| Requirement | Evidence |
| --- | --- |
| Non-public or unselected content never enters | `persona_preview_projects_only_the_public_block_and_selected_public_records`: a selected private record, an unselected public record, an unselected private record and Identity prose outside the block are absent from the projection **and** from the whole preview response; counts are reported instead. |
| Unchanged content keeps version and digest | `persona_export_writes_once_replays_and_keeps_version_for_unchanged_content`: after export v1, the same selection previews version 1 with the same `projectionSha256` and an empty diff; a second folder receives version 1 with the same digest. |
| Diff and withdrawal | `persona_diff_reports_changes_and_withdrawals_as_removals`: reclassified-to-private and forgotten records show as `removed` without text, a public revision shows as `changed` with the same `item_id`, a new Identity revision changes `/source`, `/display_name`, `/examples`; the old revision is refused as stale; version becomes 2. |
| Refused destinations, no overwrite | `persona_export_refuses_changed_previews_and_unsafe_destinations`: a Git repository, a worktree (`.git` file), this repository's own crate directory and a folder inside the open Vault are refused before anything is reserved or written; a changed digest is `revision_conflict`. The export test shows an existing file is not replaced. |
| Idempotency and unknown outcome | Same key replays (`replayed: true`) without using the token; same key with other arguments is `idempotency_conflict`. `persona_owner_resend_completes_an_interrupted_export_with_the_reserved_bytes` recreates “reserved, file unknown”: nothing happens until the owner resends, which writes the reserved bytes exactly; a reserved version is not reused for other content. |
| Identity block rules | `persona_identity_block_must_be_single_closed_and_bounded`: missing, repeated, unknown-field, two-example and control-character blocks are refused; unknown audience is refused at parsing. |
| Closed contract and sample | Contract tests `persona_corpus_agrees_with_schema_and_native_rules` (5 valid documents, 16 negative cases with the expected rule), `persona_sample_is_canonical_and_its_digests_recompute`, `persona_block_is_found_once_inside_identity_markdown`. The Python cross-check recomputes the canonical bytes and both digests independently and agrees. |

## Checks actually run

- `cargo fmt --all -- --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Full Rust test suite, run as `cargo test -p <package> --locked` for each of
  the 12 workspace members in turn (the drive ran out of space when linking all
  test binaries at once, so linked test executables were removed between
  packages): 124 suites, 1016 passed, 0 failed, 5 ignored (including the new
  ignored sample generator). The 6 new workspace tests and 3 new contract tests
  are among them.
- `python tools/schema-check/check_schemas.py` (jsonschema 4.26.0, Python 3.12
  venv outside the repository): OK, 78 schemas, `persona_documents=21`,
  `workspace_messages=168`.
- `python tools/integration/runtime_surface.py --write`: aggregate
  `f93f39ad59dd36824ee1dcdd430f01c0ee95acbb55fc6c4a67bab9117846fec4`, recorded in
  `docs/integration/RUNTIME.md`.
- Reference frontend: `npm ci --ignore-scripts`, `tsc --noEmit`,
  `npm run test:fixtures` and `npm run test:reads` (builds) passed.

## Side question: pipe-5 `sourceCarrier` (D15f / M7)

Read from code, not changed here. The pipe-5 hello descriptor
(`crates/enouia-memory-host/src/mcp/initialized/native.rs`, and pipes 6–10)
sends `sourceCarrier: "captured-source-import-1"`; the host test refuses a hello
carrying `captured-source-original-1` as a bad carrier. That matches
`MCP_INITIALIZED_AGENT_PIPE_V1.md`. `captured-source-original-1` is pipe-4's
hello value (`MCP_OWNER_AGENT_PIPE_V1.md`, qualified profile). The
`captured-source-original-1` in `contracts/next/agent-*-read-v2` is a different
field: the constant `coverage.source_carrier` inside read results. A pipe-5
client must send/expect `captured-source-import-1` in the handshake and must not
compare it with the result's `coverage.source_carrier`. Whether the coverage
constant should also be renamed is left to Morii; nothing depends on it here.

## Not verified

- The reference page was type-checked and built, not opened in a real window;
  the native folder dialog and a real Windows export were not exercised.
- No Runtime pin, adapter or product page; no downstream import of the sample.
- A real crash between ledger reservation and file write was simulated by
  editing the ledger, not by process termination.
- No private Identity or owner acceptance. Owner decisions open in the
  [contract](../contracts/PUBLIC_PERSONA_PROJECTION_V1.md) §5 remain 待 Morii 决定.
