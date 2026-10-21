# ADR-MEM-72 — Inspect and apply a saved history batch response

Date: 2026-10-21. Status: Adopted within MV-7; synthetic verification only.

Continue ADR-MEM-71 with `history_batch_preview` (read-only) and keyed
`history_batch_apply` (one local cursor step). Both accept a saved task ID and
original selected batch index. Core resolves the child through the immutable
admission and canonical receipts, revalidating all selected sources. The page
cannot choose a child Session, run, model, actor or source.

Preview reuses the extraction preview: saved response hash, original proposals,
exact quotes and selected text, current candidate status and application cursor.
It does not create records. Apply additionally requires that response hash and
an explicit expected cursor (0–32). The Provider helper checks both against the
same job snapshot it uses for application. A cursor ahead of the request returns
the current verified job without applying another item; a cursor behind the
request or a changed response conflicts. Concurrent or reopened retries cannot
advance an unselected response item.

Bind the complete apply arguments to its key with an ordinary SessionAppend
receipt before application, revising only the existing child header. Recovery
reuses this receipt and the existing candidate/cursor receipts. A failed or lost
application after admission may resume under the original arguments/key; a
changed request under that key conflicts. An admission receipt is not proof
that a candidate was published. Reads and replays still check source access,
deletion, lifecycle and saved response validity.

Apply uses the existing extraction governance, pending limits, suppression and
deduplication. It publishes at most one pending candidate and never confirms a
review or creates Canonical memory. A zero-item response may complete its local
job without a candidate. Paused jobs refuse new application. No configured
Provider, credential read or transport is required for preview/application.
Sending, human review, Runtime UI/pin adoption, real quality, MV-8 and cloud
activation remain separate. Runtime must preserve the inspected response hash,
cursor and original request key across ambiguous replies and explicitly refresh
before choosing the next step; it must not loop automatically over batches.
