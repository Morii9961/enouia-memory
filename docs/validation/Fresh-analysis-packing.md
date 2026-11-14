# Fresh analyzed plan packing — focused synthetic validation

2026-11-14. Implements [ADR-MEM-144](../adr/144-fresh-analysis-packing.md) on top
of ADR140/142/143. All inputs and roots below are synthetic.

## Actual checks

- `cargo test -p enouia-memory-init --test packing --locked`: **5 passed**.
  Covers 64/65 sources, 128/129 statements, unchanged content and quotes,
  cross-conversation support, same-source separated quotes, context index
  remapping, exact Raw/locator resolution, attachment carriers, fork lineage,
  omitted unused objects, scope watermarks and whole-preparation refusals.
- `cargo test -p enouia-memory-init --test initialization packed_fresh_authored_analysis --locked`:
  **1 passed, 22 filtered out**. Packed multi-source analysis goes through native
  capture/admission/witnessed write, closes and reopens; the original batch
  identity is reused, changed wording refuses, the two quotes still resolve,
  no duplicate memory or Owner action is created, and initializer model calls
  remain zero.
- `cargo clippy -p enouia-memory-init --lib --test packing --locked -- -D warnings`:
  **passed**. The added native initialization test also **passed** separately with
  `cargo clippy -p enouia-memory-init --test initialization --locked -- -D warnings`.
- Targeted `rustfmt --edition 2024 --check` on changed Rust files and
  `git diff --check`: **passed**.

The first packing test invocation exposed missing millisecond fractions in the
new fixture's timestamp literals; those literals were corrected before the
passing run. No production behavior was changed for that fixture error.

## Limits

These are focused checks, not a new full-workspace, Linux, schema, Runtime
surface or production acceptance result. No schema/wire/integration-surface
change is made. Previously saved full-suite evidence belongs to its own revision.

The packer refuses an individual statement above 64 dependencies and never cuts
quotes, source text or content. It does not split semantic claims, resolve
attachment files, redact surrounding Raw conversation bytes, size or execute a
production journal, grant reads or implement the fresh production CLI. Source
selection and privacy review remain the caller's responsibility. Real native
use waits for Owner merge; no real data or Provider was used in these checks.
