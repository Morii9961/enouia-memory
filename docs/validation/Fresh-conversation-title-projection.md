# Fresh conversation-title projection — synthetic validation

2026-11-17. Based on merged main `f57d84b758fa3b1f868242e4f9fab0f407601332`.

An imported conversation title containing a newline reaches the initializer's
catalog unchanged. The native catalog correctly rejects control characters in
display titles, so the entire otherwise valid plan returns `fresh.plan: Denied`.
This is a display projection problem, not a source, quote or journal budget issue.

The initializer now replaces control characters with spaces in the catalog's
optional conversation title. A title containing only whitespace becomes absent.
It preserves the exact imported title in the original Raw object and leaves
message text, locators, quotes, source identities and original input bindings
unchanged. Non-control Unicode titles retain their text; titles longer than the
existing 256-scalar limit still refuse. No title is truncated and no native rule
or capacity limit is relaxed.

## Reproduction and checks

Before the production change, the new synthetic regression
`conversation_title_projects_controls_without_changing_raw_or_expanding_limits`
failed at plan construction with `fresh.plan: Denied`.

The regression covers a newline/tab title, an all-whitespace title, an ordinary
Unicode title and the existing oversized-title refusal. For accepted cases it
forms through the native initializer, closes and reopens the synthetic Morii
root, checks the stored catalog title, resolves the original exact quotation,
and compares archived Raw bytes with the unchanged input object.

- `cargo test -p enouia-memory-init --test fresh_driver --test packing --locked`:
  **15 passed, 0 failed, 1 ignored**. The driver target contributed 10 passes
  (66.81 s), including the new native formation/reopen regression; packing
  contributed 5 passes (0.25 s). The existing opt-in trusted CLI test stayed
  ignored and was not rerun for this localized change.
- Scoped Clippy for the initializer library, `fresh_driver` and `packing`, with
  `--no-deps --locked -- -D warnings`: passed.
- Targeted rustfmt and `git diff --check`: passed.

No full-workspace, Linux or schema cross-check was run. The Runtime surface did
not change, so its checker was not rerun for this initializer-only correction.

## Scope

Only the initializer's display-title construction changes. There is no schema,
validator, native admission, Runtime integration surface, command or stored-format
change. Existing compatible native readers already accept the projected title.
The ADR146 compatible-reader gate remains independent.

All fixtures are synthetic. This report establishes no real-corpus capacity,
real formation, independent-media recovery, Runtime product acceptance, Provider,
read grant, deployment or deletion result. The full original input and original
run must be preserved. The correction must merge before real use.
