# ADR-MEM-164 — Time-derived attention schedule and review suggestions

Date: 2026-11-22. Status: Accepted (roadmap items, Morii 2026-11-19); synthetic
validation.

## Context

The roadmap (IMPLEMENTATION_PLAN §15) commits to two features:

- time-derived fade and archive events;
- review suggestions (Runtime G24).

A memory's attention state is computed, not recorded. `attention_state` compares
its latest source capture time with the profile's `fade_after_days` and
`archive_after_days`, and profile or Morii protection overrides both. Nothing in
the vault records the moment a memory starts to fade, so the activity feed has
no such entries.

## Decision

Two read-only System projections on `SegmentedMemoryReader`, rebuilt from the
freshly authorized set. Neither writes a revision or a cache.

- `memory_attention_schedule_native(request, query, cursor)`:
  - **Window.** `query` is a half-open window `[from, until)` of at most 366
    days.
  - **Events.** For every selected, unprotected memory it lists the `fading`
    and `archived` transitions at capture time plus the profile's day count,
    when they fall inside the window.
  - **Event fields.** Each event has the memory, its title, the transition,
    `at`, and `past` (whether `at` is at or before the evaluation time).
  - **Paging.** Events are sorted by time, memory and transition, and pages
    follow `limit`, `max_bytes` and a cursor bound to the projection header.
  - **Protection.** The page reports how many selected memories are protected;
    protected memories have no transitions.
- `memory_review_suggestions_native(request, query)`:
  - **Horizon.** `query.horizon_days` is between 1 and 90, counted from the
    evaluation time.
  - **Reasons.** `archives_soon`: a fading memory is archived within the
    horizon. `validity_ends_soon`: its recorded `valid_time.until` falls within
    the horizon.
  - **Suggestion fields.** Each suggestion carries its reasons, both times
    when present, the current attention state, and `due`, the earliest of
    those times.
  - **Ordering.** Suggestions are sorted by `due`. `items` holds the soonest
    within `limit` and `max_bytes`, and `total_in_horizon` counts all of them.

Both projections use the catalog's selection, filter and sensitivity ceiling, so
only authorized current memories appear. Withdrawn, forgotten and deleted
memories never do. The header notes state that the transitions are derived, not
recorded, and that suggestions are time-based, not content judgements.

These are separate projections and not entries in the activity feed or change
cursor. Feed entries carry native publication proofs of commits. A derived
transition has no commit, so mixing the two would blur what was recorded and
what was computed.

## Consequences

- Runtime can draw upcoming and past fade and archive moments on the timeline,
  and can show a "worth a look" list with reasons. Whether to review, correct or
  protect a memory stays Morii's decision, through the existing Owner actions.
- The types are host-level `Serialize`/`Deserialize` structs in
  `automation::catalog::attention_schedule`. No contract schema, persisted
  format, IPC command or wire change.

## Verification

A synthetic library with four memories, using profile fade 30 and archive 180:

- fresh, with validity ending in 5 days;
- fading, archived in 10 days;
- archived 20 days ago;
- protected by the profile.

Checks:

- The schedule lists exactly the six expected transitions, in order, with the
  correct `past` flags.
- Paging two at a time reproduces the same list and ends without a cursor.
- A 31-day window returns two transitions; a window over 366 days is refused.
- With a 14-day horizon, review suggests two memories: first the validity end,
  then the archive. A 7-day horizon suggests one; a 91-day horizon is refused.
