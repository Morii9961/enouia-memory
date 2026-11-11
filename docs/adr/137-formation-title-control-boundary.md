# ADR-MEM-137 — Control-free fallback titles for quoted formation

2026-11-11. Morii authorized the worker title fix identified in ADR135's handoff.

## Decision

When an analysis title is not contained in its quoted evidence, `formation_seed`
uses the first nonempty, trimmed span separated by control characters, limited
to 60 Unicode characters. This matches the existing legacy initializer fallback.
The result remains a literal substring of the quote. Newlines, CRLF and tabs in
the quote cannot become control characters in the generated title.

Keep the original content and evidence byte-for-byte. A supplied title already
contained in the quote still follows the existing validation, including refusal
of control characters or excessive length. Quotes without a usable title remain
refused. No new helper, dependency, wire field or policy is added.

This shared native producer serves workers and initialization. Regenerate the
Runtime surface and log the adoption requirement with the implementation. No
Runtime checkout, real model, owner data, write/capture grant or MV stage change.
