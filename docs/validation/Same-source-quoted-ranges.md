# Same-source quoted ranges — synthetic validation

2026-11-13. ADR-MEM-143, stacked on the supplied-analysis initializer
(ADR-MEM-142). All test sources and temporary native roots are synthetic.

## Behavior

Native admission accepts distinct Unicode-scalar quote ranges from one source
when paired record fields retain the exact references, text and speaker.
Dependency closure and origin uniqueness still count that source once. Exact
repeats, repeated unquoted sources and distinct sources copying one origin still
refuse. Quote, source, document and evidence budgets remain unchanged. The
initializer retains the supplied ranges without concatenating or dropping them.

## Reproduction and checks actually run

Before implementation, the new initialization case with two short passages
separated by more than 500 scalars failed preparation with `Denied`. After the
fix, `cargo test -p enouia-memory-init --test initialization fresh_authored
--locked` passed all **5** selected cases. Native write and reopen retain both
quotes, one source dependency, the authored content and no Owner actions. A
changed support quote conflicts with the original enqueued artifact, and model
invocations remain zero.

`cargo test -p enouia-memory-contract --test ui_records same_source_quotes
--locked` passed its **1** selected case: separated ranges including an emoji,
exact duplicate and unquoted-input refusal, mismatched reference pairs and a
shape-valid forged range which fails source resolution. An initial fixture
iteration failed because its entity still cited the old quote; the synthetic
entity reference was aligned without weakening production validation.

Additional checks:

- `cargo fmt --all -- --check`: passed.
- Independent Python `jsonschema` 4.26.0 schema cross-check: **79 schemas** and
  the existing fixture corpora passed. Wire schema fields are unchanged.
- `python tools/integration/runtime_surface.py`: passed with aggregate
  `39cc2a820fd132770397e3cef37674b8fca17280c8977bcd161622a43ad54c52`.
- Full workspace test: initial run stopped at the existing Host correction-worker
  test with `Busy` from its read after formation. The isolated failing case
  passed, and the complete workspace rerun passed without another production
  change. The initial failure is retained here rather than counted as a pass.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.

The source tree uses the pinned toolchain with Cargo offline. The full test and
Clippy runs disable incremental compilation. Runtime adoption is documented in
[the integration log](../integration/RUNTIME.md); no Runtime checkout is changed.

## Limits

This extends semantic acceptance of an existing wire shape. Older native
evaluators can refuse these records; Runtime must pin the matching revision and
surface before writing them. It does not prove semantic entailment of a
paraphrase or completion of corpus analysis. No real conversation, attachment,
Provider, production Vault, independent backup, deployment, deletion, read grant
or Owner acceptance was exercised. Production use awaits Owner merge.
