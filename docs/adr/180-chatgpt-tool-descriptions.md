# ADR-MEM-180 — Tool descriptions worded for ChatGPT

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

All five read tools shared one description written for local Agents: "Scoped
automation-root read", coverage notes and protocol terms. ChatGPT chooses tools
by their descriptions, so this one never said which tool answers a question,
which lists, which reads one memory in full, or which shows the original text.

## Decision

On the remote service, the ChatGPT layer (ADR-MEM-178) replaces the
description of `session_start`, `memory_recall`, `memory_search`,
`memory_read`, `memory_source` and `memory_related` with one sentence or two
about when to use the tool and what it returns. The honesty notes stay:

- memories record what Morii said or what was inferred, not verified facts;
- suggestions are not facts;
- reads change nothing.

Local Agent protocols keep their descriptions.

## Verification

In `remote_daily_process`, every listed tool has its own description, and the
existing flow passes unchanged.
