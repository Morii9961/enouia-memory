# ADR-MEM-142 — Form supplied fresh analysis without legacy candidates

2026-11-12. Fresh initialization requires newly distilled content with exact
support and truthful analysis provenance. The legacy route keeps one source
quote as content and labels its producer as scripted. It cannot represent this
input without losing the distillation or mislabelling its origin.

## Decision

Add a pure `Plan::build_analyzed` entry for an explicit `AnalyzedBatch`. Each
statement supplies distilled content, the existing formation dimensions and
separate event/validity times, one primary quote, and further supporting quotes.
The batch records its analysis actor, prompt version and optional known model
version. Unknown model identity remains null. It calls no analyzer or Provider.
JSON decoding grants no selection, confirmation, activation or read authority.

Reuse `Plan::build` source capture, capacity and original-locator checks, then
bind every supporting quote to the same captured sources. Admission currently
allows one range per source: duplicate-source ranges refuse rather than being
silently dropped. The caller must explicitly supply one adequate exact range
within the existing 500-character bound, or keep that record unprepared.
Context-only dependencies remain context; a source explicitly quoted as support
is promoted to support. Sensitivity covers all input dependencies. Assistant
support cannot become a clear Owner self-report. Inference remains staged under
the existing evaluator, without a per-item confirmation action being invented.

Use the supplied content/title and provenance in the ordinary AdmissionInput
and record fields. Validate the complete input under the existing contract.
Retain the whole authored batch as the hash-bound analysis artifact, so changes
to content, quotes or provenance conflict with the existing run identity. The
initializer ledger distinguishes its zero model calls from the authorship of
supplied analysis. Existing synthetic callers retain their scripted route.

No contract/schema or Runtime integration surface changes are needed. The same
native evaluator, authenticated bootstrap adoption, journal, receipt lookup and
unknown-outcome barriers remain responsible for any later write. This entry is
preparation, not independent proof that a paraphrase follows from its quotes.

## Scope and verification

Use synthetic multi-source content, a long untruncated distillation, exact quote
refusals, role and sensitivity boundaries, changed-analysis resume conflicts,
and native write/reopen checks. Production input waits for Owner merge approval.
Bounded full-corpus packing, attachment review, the fresh production driver,
corpus completion, actual Vault acceptance, backup and remote read scope remain
separate work. Do not revive legacy candidate contents or call this completed
initialization. No deployment, deletion, new MV stage or Runtime edit is included.

Validation: [focused authored-formation checks](../validation/Fresh-authored-formation.md).
