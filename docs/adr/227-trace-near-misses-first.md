# ADR-MEM-227 — A bounded recall trace lists the near misses first

Date: 2026-11-27. Status: Accepted. Changes the order of
`RecallTrace::excluded`; what is selected does not change.

## Context

A recall considers every authorized memory. Hosts return at most 128
exclusions (`trace.excluded`) and count all of them by reason in
`trace_coverage`. The selector pushed exclusions in candidate order while it
filtered (`not_adopted`, `no_cue` and so on) and appended the memories that
had a cue but did not fit (`budget`) last. On Runtime's real library a
query considers 351 memories; about 130 are filtered and about 220 have a
cue but are outranked. The first 128 entries were therefore nearly all
filtered ones, and the memories that just missed the top 8 had no listed
reason. Reading a trace, Runtime could not tell "nothing pointed to it" from
"it did not make the top 8" for a given memory.

## Decision

- The selector lists the `budget` exclusions first, in rank order (best
  first, the order the selection used), then the filtered ones in candidate
  order. A bounded trace now shows the near misses before anything else.
- `budget` still covers both an item limit and the byte budget; the trace
  shape, the reasons and `trace_coverage` are unchanged.
- Where `trace_coverage` lives: in `recall_preview` and in the Agent's
  `memory_recall` it is a top-level field next to `trace`, not inside it:
  `excluded_in_scope`, `excluded_returned`, `complete` and `reason_counts`
  (all exclusions by reason, including those not listed).

## Verification

`a_bounded_trace_lists_the_near_misses_first` (contract): twelve memories
name 茶 and eight fit; the four that did not fit come first, in rank order,
then the twenty with no cue. Host recall tests are unchanged.
