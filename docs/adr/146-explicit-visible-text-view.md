# ADR-MEM-146 — Explicit complete visible-text views of mixed messages

2026-11-16. Morii approved a bounded visible-text formation entry after actual
intake exposed mixed messages refused as Partial. This amends ADR145; it starts
no MV stage, semantic extractor, attachment analyzer or Provider integration.

## Decision

Keep the original imported message's `completeness: partial`, exact Raw, source
identity/revision, metadata, speaker, branch and every attachment status. Only an
explicit `visible_text_only: true` source selection declares a complete view of
all string parts of that ChatGPT content object. No implicit promotion to Complete.
The reserved source extension `memory-init.visible-text-view` records precisely
`{"version":1,"scope":"all_string_parts"}`. It is selection data, not authority.

Native import must check the declaration against the original hashed Raw object:
the locator names a mixed content object, every string part is retained in order
with the parser's exact newline join, at least one non-string part exists, and
the upstream message/conversation identity and speaker match. Empty/image-only
views, cut/changed text, forged locations/identities, unsupported scope and missing
Raw refuse. Native admission accepts a validated declared text view while keeping
Partial visible. Exact quoted text and existing dependency/role/subject/sensitivity
rules remain required. Unselected Partial, metadata-only and unavailable sources
still refuse. Missing/unreviewed attachments remain explicitly missing/unreviewed;
this view never proves a visual claim, fetches a URL or grants attachment reading.

Add explicit per-statement `context_quotes` to the fresh manifest: source-table
index, exact quote and scalar start/end. Each must name an existing dependency;
the range is never promoted to support even if another range from that source
is separately direct evidence. Resolve it before any write and bind
the native quoted references into the existing analysis artifact. Empty defaults
preserve existing callers; supplied ranges may never disappear during packing.

Allow a caller-selected, hash-bound JSONL authored-record object and explicit
statement-to-record indexes. Preserve its exact bytes as a normal Raw analysis
artifact, not a database or substitute for native formation. Validate the selected
object's ID/revision/content/title boundary and bind object hash, line index and
record digest into each plan's analysis artifact. This retains the original
analysis provenance, layer, fine-grained time basis, context, cautions and reviewed
attachment links without inventing Morii confirmation or bloating every journal
with copies of the entire authored record. The ordinary native writer still forms
each record and issues its own receipt. Artifact changes conflict with the run.
The complete selected analysis file is retained, including any unselected lines;
this is preservation, not redaction or permission to expose its contents.

## Verification and adoption

Synthetic tests must cover opt-in and default refusal, exact Unicode text/ranges,
unchanged Partial/attachments/Raw, native capture/reopen/query, forged text/Raw/
identity/scope refusals, context-only closure and hash-bound authored metadata.
Measure the updated whole-journal bound without raising limits or pruning history.

The admission surface changes: Runtime must adopt a compatible pin before opening
these new text-view records; old evaluators can refuse replay. This is a new
requirement for this increment, not a redo of the already completed ADR143 adoption.
No Runtime checkout, UI, IPC command or read grant changes here. A reviewable PR
must be merged before using the new capability on real inputs. Actual full-plan
capacity and real Morii formation/read/source acceptance remain pending.
