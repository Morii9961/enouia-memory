# ADR-MEM-60 — Bounded strict JSON for offline quality assessment

Date: 2026-10-14. Scope: MV-7-H2k, offline evaluation only.

The evaluator's duplicate-key reader still admits NaN/Infinity and floating
overflow, and sufficiently deep input raises an uncaught RecursionError. A
hash-bound task containing a nonstandard number can be reported as structurally
matched; this does not describe JSON that the native builder can produce.

Use the same strict bounded reader for suites, selected source documents and
embedded analysis input. Preserve existing byte limits and JSON byte encoding
support. Before recursive parsing, scan decoded text with string/escape awareness
and cap container depth at 128. Brackets in quoted source text are data.

Reject NaN, Infinity, negative Infinity and numbers whose floating conversion is
not finite. Keep finite numbers, nulls, booleans and ordinary strings unchanged;
retain duplicate-key rejection, including escaped-equivalent keys. Translate a
decoder recursion failure into a bounded-input ValueError so the CLI emits its
ordinary invalid-input diagnostic, no partial metrics or traceback.

Validate the legacy source archive shape before navigation: an array of
conversation objects, string message references, a selected mapping object and
a selected message/content object with a parts array. Malformed conversation
scalars and mapping arrays previously raised uncaught AttributeError. Preserve
mixed text/attachment metadata in valid parts arrays and bind its original scope
hash; shape validation does not download, normalize or interpret attachments.

This is syntax/resource validation, not native authentication, model judgment,
parent-chain verification or owner acceptance. Real data/model calls, Runtime
product adoption and later stages remain outside this slice. There is no Rust,
wire/storage contract or dependency change.
