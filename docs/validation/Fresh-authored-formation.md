# Fresh authored formation — focused synthetic validation

2026-11-12. ADR-MEM-142. Scope: `enouia-memory-init` supplied-analysis
preparation and its existing native initialization route. All content is synthetic.

## Change

`Plan::build_analyzed` preserves supplied distilled content and title separately
from primary/supporting quotes. Explicit actor/prompt/optional model provenance
is hash-bound to the analysis artifact and carried into formation fields and the
ledger. Context dependencies and support retain distinct roles. Input sensitivity
covers dependencies. The initializer itself calls no model or Provider.

## Checks actually run

- `cargo test -p enouia-memory-init --test initialization --locked`:
  **21 passed**, including four new authored-formation tests.
- Targeted `rustfmt --edition 2024 --check` for the changed Rust files: passed.
- `CARGO_NET_OFFLINE=true CARGO_INCREMENTAL=0 cargo clippy -p enouia-memory-init
  --lib --test initialization --locked --no-deps -- -D warnings`: passed.

The new cases write through the existing native evaluator/witnessed writer,
reopen the synthetic root, resolve both original quotes, retain content longer
than 500 characters and a distinct title, preserve unknown model identity, and
prove idempotent enqueue and changed-content/provenance conflicts. They also
check missing/unresolvable/duplicate-source quotes, Assistant support rejected
as an Owner self-report, sensitive support propagation, staged inference,
content limits, closed JSON decoding and zero fabricated Owner actions.

Two initial test iterations found a fixture collection-type mismatch and a
missing serde default for omitted empty context sources; both were fixed before
the passing run. Cargo reported incremental-cache hard-link fallback warnings
in the test run; tests still passed. Clippy used incremental compilation disabled.

## Limits

This does not measure semantic entailment or corpus quality. No real conversation,
attachment, Provider, production Vault, remote deployment, independent backup,
Runtime integration, deletion or Owner acceptance was exercised. Full workspace
checks and schema/Runtime surface regeneration were not run: no contract,
validator schema, dependency or registered Runtime surface changed.

Production use awaits Owner merge approval. Complete source packing, attachment
handling and a fresh production driver remain required before native full-corpus
initialization; the legacy candidate route is not reused as semantic input.
