# ADR-MEM-101: Witnessed process, history and conversation reads

Date: 2026-10-27. Status: implemented; synthetic validation.
Builds on ADR93/94/100. Concurrent MCP decisions are not imported.

## Decision

Keep schema 2, accepted ADR93 record fields and new-empty-root layout
`automation-segmented-host-v1`. Add transport-neutral native read-2 ports on
`SegmentedMemoryReader`; no default Workspace/MCP route or Runtime adoption.
Catalog selections now emit `qualified-family-catalog-3`, version 3. Catalog
rows require effective sensitivity and recorded formation reason; graph nodes
require effective sensitivity. Old DTOs remain parseable under their earlier
version, but old cursors cannot continue under this reader.

All ports resolve fresh current authorization, lifecycle, exact source/entity/
parent dependencies and deletion barriers before titles, counts, histories or
relations. Historical revisions additionally require a visible current identity,
the original exact active policy and still-current original dependencies. A
previously visible historical title cannot survive current deletion/revocation.
Effective sensitivity is the recorded envelope whose source, entity/alias and
recursive-parent floors were enforced at publication and checked on read; it
is neither the caller's ceiling nor the policy's maximum.

Derive activity only from witnessed linked capture/admission commits and their
actual receipts. Bind each event to commit ID/hash, actual actor and publication
clock, keeping formation/change/knowledge/adoption separate. Reads never invent
jobs, owner confirmation, clock-triggered fading or archival. Deleted records
have no visible historical title or deletion event disclosing that title.
The activity scope is current eligible dependencies, not an immutable audit of
everything the owner once saw. Time-derived events are explicitly unavailable.

Derive a private commit-position index during publication/replay. Change queries
bind an ancestor snapshot, exact grants/ceiling and query hash; they return
current authorized appended IDs without serializing full catalog pages. Return
an explicit reload on revisions, policy/deletion/metadata barriers, unknown
ancestry or more than 100 changed IDs. Never silently truncate an exact delta.
A changed snapshot can invalidate projections while exposing zero changed IDs.
Do not expose global commit sequence numbers or counts of hidden changes.

Conflict groups mean recorded counter-source overlap, not independently verified
contradiction. Return the selected record and current eligible counterpart
records whose evidence cites a declared counter-source. Structural revision
diffs compare only adjacent eligible revisions. Selected neighbors distinguish
recorded derivation/reversion/revision/correction/entity/conversation relations
from qualified theme suggestions. Machine correction never means corrected by
the owner. No semantic-neighbor model or embedding is introduced.

Formation explanations copy interpretation, rule/prompt/model versions,
mode/batch and actual recorded note. Missing notes stay null; notes remain
machine annotations even if their text claims owner confirmation.

Session ports return current authorized conversation metadata, bounded captured
turn fragments and selected Unicode-scalar source excerpts with exact hashes.
Historical fragments retain their own sensitivity floor; a public source cannot
declassify a private fragment's branch/turn metadata. Caption/participants require
their current dependencies. Inaccessible parent source links are omitted without
hidden placeholders. Distinct turn count and source/branch-row count are separate;
platform completeness is never claimed. Source metadata excludes duplicated raw
manual assertion/agent submission bodies; extensions are explicitly untrusted.
Unavailable, deleted and ungranted selections share the same empty result.

## Adoption and evidence

Every process result carries a versioned algorithm, snapshot/query/profile/
parameter/permission binding, fresh authorization time and declared scope.
Pagination freezes evaluation time, never current permissions. Row/byte bounds
refuse a nonprogressing page. Read results are disposable and publish no record.
Runtime must adopt the exact manifest and ports, restart earlier cursors, render
unavailable time-derived events and preserve qualified relation/reason labels.
Actual owner ledger execution remains the separately authorized G08 slice.

See [contract](../contracts/UI_V2_PROCESS.md) and
[validation](../validation/UI-v2-process.md).
