# ADR-MEM-55 — Owner-selected semantic proposal drafts

Date: 2026-10-13. Scope: MV-7-H2f, optional native draft construction.

A validated semantic response remains a model interpretation. Add an explicit
owner choice bound to its task, dispatch, response hash and change index. Require
the exact proposed content even when the owner retains the original statement.
Revalidate the whole saved response and all selected source revisions and local
permissions. Drafting is read-only and never sends or approves.

Use the existing Proposal/Origin contracts. OwnerManual names the owner submitting
a proposal, not the author or evidence class of the underlying interpretation.
Retain model statement/explanation, task/input/dispatch/response binding, exact
quotes/offsets and original evidence classes as bounded inert JSON in reason.
Do not create a manual assertion or invent an ExtractionJob/run for this task.
EvidenceSpec cites original Source revisions; governance resolves their actual
classes and anchors. Keep the strictest sensitivity across all selected context.

Draft facts as uncertain (conflicts as disputed), preferences as tentative, and
refuse preferences with no user evidence on the selected change's current side.
Summary points are not candidate statements. A supersession draft is allowed only
for a validated correction/cancellation, with an explicit current active intact
target on the same exact topic and an owner-supplied BusinessTime (including an
explicit unknown). Never derive business time from message time. Conflict and
uncertain branch differences cannot automatically become supersessions.

The caller may publish one draft through existing propose_at_head, using a stable
key bound to the exact owner choice; refresh after head conflicts. This creates a
pending candidate only. Ordinary trusted owner plan/confirm remains necessary.
The reason string is provenance data, not an approval or authenticated standalone
receipt. Drafts and public structs are not security capabilities; a native host
must enforce owner/lock and content review when publishing, like existing drafts.

Source purge with dependents must also purge candidates citing affected source
revisions, including model interpretation text and reason quotes. Other canonical
memories retain the existing explicit broken-provenance treatment. Runtime product
UI remains Runtime-owned. No new wire command, schema, provider call or stage.
