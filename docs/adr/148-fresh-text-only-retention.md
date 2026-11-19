# ADR-MEM-148 — Explicit text-only fresh initialization

2026-11-16. Morii clarified that the new library retains text memories and text
sources, without copying original image/attachment files into the Vault. This
supersedes the proposed attachment-chunk work. It amends ADR145/146, starts no MV
stage and adds no Provider, extraction, read grant or deletion capability.

## Decision

Add the explicit run-wide fresh manifest selection
`"attachment_retention":"metadata_only"`. Every parsed attachment slot must still
be bound, with `original:null`. Conflicting original-file selections refuse before
input-object reads; no attachment file is opened, hashed, chunked or archived. Unknown
retention values refuse. The absent field defaults to `selected_originals`, keeping
existing callers and original-file validation unchanged. Selection and analysis
still bind the exact manifest bytes; changing retention cannot resume an old run.

Keep each parsed attachment's name, claimed media type, availability and inert
external reference. Add the persisted attachment extension
`memory-init.attachment-retention` with version 1, `payload:"not_selected"` and
`parsed_availability`. The carrier's existing `missing` availability means no
payload was present in the received JSON; it is not a claim that an external
original was searched for or lost. Metadata-only slots are counted separately
from unavailable originals in preflight. No detected type or verified original
hash is invented. Embedded attachment payloads refuse this mode rather than being
silently stripped from Raw. This extension is descriptive selection data, not
Morii authority or a visual evidence claim.

The native memory fields remain text. Complete selected conversation JSON and
the authored analysis JSONL remain exact, bounded textual Raw objects in the
Vault, preserving quote checks, speakers, branch, original locators, surrounding
context, original analysis and temporal/subject/sensitivity metadata. This is not
a redacted or summary-only archive. Attachment references in that text remain
references; metadata-only retention does not make their original bytes available.
Previously authored visual analysis keeps its provenance and uncertainty; the
new library cannot recheck an image after its external original is removed.

No Raw, plan or whole-journal capacity limit is raised. Existing native formation,
close/reopen, source queries, idempotence and original-packet uncertainty barriers
remain in force. Original external data is not deleted by initialization. Any
later cleanup follows actual new-library acceptance, independently verified
backup/restore, exact-path review and Morii's last deletion confirmation.

## Delivery boundary

This is initializer-only selection and additive descriptive attachment metadata.
No contract enum/schema, native admission rule, Workspace command, Runtime pin or
UI changes. ADR146's compatible-reader adoption requirement still applies.
Verify synthetic dry-run, all-slot closure, conflicting/default selections,
multi-batch formation, reopen, quoted sources, run conflicts, journal capacity
and the actual trusted bootstrap CLI route. Reuse unchanged recovery evidence.
Deliver a reviewable PR and wait for merge before using the new mode on real data.
