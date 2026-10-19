# ADR-MEM-66 — Read-only history planning for the embedded host

Date: 2026-10-19. Status: Adopted within MV-7.

## Decision

Expose `Workspace::plan_history(HostSurface, &HistoryPlanRequest)` as a native,
read-only host method. Runtime can wrap this method in its own native adapter;
it must derive the surface from the calling window, never page arguments.
Quick Search is refused. The existing 47-command page protocol is unchanged.

The request names an observed Vault ID, commit ID, import ID and import revision,
an explicit nonempty selection of adapter unit indices, a branch selection
(`all` or `export_selected`), and bounded planner limits. The Core reads only
archived bytes, verifies their hash/size and adapter identity, binds selected
messages to current source revisions, and checks source-read policies and
logical/physical deletion barriers. No file paths or raw JSON are accepted.

Return a versioned JSON observation with selected coverage, unknown times,
parser/attachment/graph gaps, source bindings and branch-preserving batch
summaries. Observed times are not completeness or truth claims. Batch IDs are
deterministic hashes bound to the Vault, commit, request and source revisions;
they are observation identifiers, not authorization or durable extraction jobs.
No source text, archive member names or upstream titles/identifiers are returned.

Limits: at most 128 selected units, 4,096 selected messages, 256 messages per
batch, 16,384 expanded path references, 8 MiB archived input, 16 MiB expanded
ZIP input, 256 KiB serialized result, and 16,384 current source records inspected.
Excess is refused without partial output. Message counts are not token budgets.

The lifecycle read gate excludes lock/close/switch. A moved commit, policy or
deletion watermark invalidates the observation before return. Hosts discard
results on lifecycle changes and ignore late replies; they must observe and
plan again after changes. Nothing here caches permission or grants a future
write. Sessions, jobs, candidates, canonical records and the commit are unchanged.

## Consequences

No new dependency, Runtime checkout change, product UI, Provider call, personal
history activation, MCP or MV-8. Runtime must adopt the new pinned Core and add
its own scoped native adapter and UI. Batch materialization and subsequent
extraction/review remain separate slices. The size limits deliberately do not
claim support for full-scale history; an oversized archive needs a future
bounded archive reader rather than silently truncated planning.
