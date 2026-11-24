# ADR-MEM-203 — Theme refusals name each member; no panic on imported members

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

ChatGPT builds weekly themes (ADR-MEM-174) from what `memory_search` returns,
and that can include memories the connector did not submit, such as imported
ones. Two problems came up.

- **Generic refusal.** The refusal said only "every member must be a current
  memory this connector submitted", so ChatGPT could not tell which member
  to drop.
- **Panic.** `submit_theme` read each member's record fields with `unwrap()`.
  A member without record fields would have panicked and ended the service
  process.

## Decision

- `submit_theme` treats a member without record fields or quoted evidence as
  an invalid member and returns `Invalid` instead of panicking.
- `ConnectorWriter::theme_problems` gives one reason per member that cannot
  join:
  - it is not a current memory visible to the connector;
  - it was not submitted through this connector (another domain, subject or
    project);
  - it has no quoted evidence.
- The connector reports, in order of preference:
  1. the theme's own field problems (ADR-MEM-181);
  2. failing those, these member reasons;
  3. failing those, the previous general sentence.

## Verification

- `connector_formation::chatgpt_themes_synthesize_daily_memories_across_windows_of_its_domain`
  checks a theme with one valid member, one from another connector and one
  unknown ID. It gets exactly two reasons, naming those two IDs. Two valid
  members get none.
- `remote_daily_process` (real binary) submits a theme that includes a
  baseline memory. The refusal has one problem, and it names that memory.
