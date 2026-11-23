# ADR-MEM-182 — Recent-memory search for ChatGPT

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The connector usage guide asks ChatGPT to look at recent memories once a week
and submit a theme if one recurs. The short `memory_search` (ADR-MEM-178) had
no time filter. ChatGPT could only page through everything, newest first, and
guess where the week ended.

## Decision

The short `memory_search` takes an optional `days` from 1 to 366. The layer
then sets the catalog filter to:

- `time_axis: "learned"`;
- `time_window: {from: now − days, until: null}`.

The result is the memories Memory learned in that period, on the same
authorized, filtered and paged search. Any other value of `days` is refused
with `invalid_arguments`. The full envelope already supports any window and
is unchanged.

## Verification

In `remote_daily_process`, on both roots:

- `{"days": 1}` returns exactly the two memories submitted in the run, not
  the fixture history learned a month earlier;
- `{"days": 0}` is refused.
