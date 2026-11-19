# ADR-MEM-152 — Native System recall preview

Date: 2026-11-19. Status: Accepted; synthetic validation only.

Runtime's Context surface and recall lab need to show what recall would pick
from a segmented Vault opened through the registered read-only entry. Before
this change, segmented recall existed only on `SegmentedAgentHost::recall`, which
requires an enrolled Agent reader and stores the latest result for
`context_trace`. The native System reader (`SegmentedMemoryReader`) had pages,
facets, stats, graph, activity, detail and source reads, but no recall. Runtime's
Context surface therefore still used Workspace v1.

## Decision

Add `SegmentedMemoryReader::recall_preview_native(profile, query,
max_sensitivity)`. It opens the current read through the native System actor,
under the caller's sensitivity ceiling and the current policy, exactly as the
other native catalog reads do. It returns:

- `effect: native_read`
- `trace`: the `RecallTrace`, with excluded entries capped at 128
- `trace_coverage`: excluded in scope and returned, `complete`, and
  `reason_counts`
- `rendered_context` and `items`
- `complete_in_scope`

Agent recall and the preview call one shared function. It projects each
authorized current memory, renders it, runs `select_catalog_recall` and builds
the same coverage. For the same authorized rows, profile and query, the two
results have identical selection, exclusion reasons, rendered bytes and text.
The preview adds no `request_id` or `scope_hash`. It writes no Capsule, stores
no context and needs no Agent enrollment. A locked reader, an invalid profile
or query, or a result over the document bound is refused.

## Boundaries

No new persisted format, wire command, IPC method, grant or Provider call. The
Workspace surface stays at 61 commands; only the Rust source digests of
`catalog.rs` and `segmented_agent.rs` change in the Runtime surface manifest.
Runtime adopts the method by moving its pin and forwarding it from its native
adapter. Semantic suggested edges, time-derived fade or archive events and review
suggestions remain later work.

## Verification

Synthetic segmented Vault with 40 records: for three queries (current text, a
CJK term under a 4 KiB budget, and browse), the preview equals Agent recall in
trace, coverage, rendered context, items and completeness. The Vault's bytes are
unchanged afterwards. Invalid queries and a locked reader are refused, and unlock
restores the reader. The existing Agent recall, pagination and context-trace
tests are unchanged and pass.
