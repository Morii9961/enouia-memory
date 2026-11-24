# ADR-MEM-183 — Conversation receipts name the memories they formed

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

A `memory_submit_conversation` receipt listed the capture, the queue and each
job's state and receipt count, but no memory IDs. ChatGPT could not tell which
memories it had just created. To cite them in a theme, or to confirm what was
kept, it had to search and guess.

## Decision

`ConnectorReceipt` gains `memories`: the current `memory_id`, revision and
title of every memory that meets two conditions:

- it was formed under the submission's capture window policy;
- it cites one of the submission's captured sources.

A replay reads the same sources and returns the same list. Preflight and
window-needed receipts return an empty list. A theme built over these memories
has its own synthesis policy, so it is never listed. The read uses the
connector's sensitivity ceiling and Morii's full-history System scope, as the
theme path already does.

## Verification

In `remote_daily_process`, on both roots:

- The first conversation's receipt names one memory with its title, and the
  replay names the same one.
- The two receipts together name exactly the two memories the next search
  finds.
- After a theme over them forms, a replay of the second conversation still
  names only its own memory.
