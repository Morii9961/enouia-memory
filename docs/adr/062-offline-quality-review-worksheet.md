# ADR-MEM-62 — Read-only offline quality review worksheet

Date: 2026-10-16. Scope: MV-7, companion to ADR-MEM-54 and ADR-MEM-59.

## Decision

Add an explicitly selected-suite command that returns a bounded Markdown review
worksheet to stdout. Keep the evaluator's counts-only report unchanged. Reviewers
can inspect the selected source scope, response statements and exact review paths,
required semantic units, and existing judgments together. Include a structured
view of optional saved analysis input; its original UTF-8 hash remains separately
labelled, and the view is not a reproduction of the original byte formatting.

Validate the entire suite through the existing evaluator first. Bind the rendered
suite and every source scope to the corresponding validated JSON-value hashes;
reject a changed read before emitting any output. This is consistency checking,
not authentication, native revalidation, or a filesystem transaction.

All imported data, identities, statements, reasons, and original text go inside
JSON code fences. Only static headings and numeric case positions become Markdown
structure; fixed status translations and calculated counts may appear as prose.
Preserve empty judgments, pending review, unknown units, provenance
declarations, native_validation=not_rerun and overall_model_quality=null. Never
infer judgments, construct approval, or rewrite the selected suite.

Default output budget is 512 KiB of UTF-8, with an explicit hard maximum of 2 MiB.
Reject the whole output on budget or input error; stdout stays empty. The command
does not save files, search private directories, read a Vault, call a Provider,
fetch attachments, or write memories. The worksheet contains selected original
content and is private material; any explicit redirection/save belongs to the
caller. It is an offline document, with no Runtime product UI or wire changes.

## Evidence

Implementation and synthetic validation are recorded in
[the worksheet report](../validation/MV-7-quality-review-worksheet.md). Actual
human review, model fidelity, omission, and owner acceptance remain separate.
