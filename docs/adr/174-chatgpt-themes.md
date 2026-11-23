# ADR-MEM-174 — Themes synthesized by ChatGPT across daily memories

Date: 2026-11-23. Status: Accepted (Morii chose ChatGPT-submitted themes,
2026-11-23; Runtime R5 B2); synthetic validation.

## Context

The real Vault has no themes, so Runtime's theme lanes are empty.

- **No synthesis entry.** Theme synthesis existed only in the legacy automatic
  loop. The segmented Vault had no synthesis entry.
- **Parents in one policy.** Three things forced every parent of a derived
  memory into the derived memory's own policy: the contract (`p.policy !=
  input.policy`), the Vault's parent closure, and a separate random access
  domain per capture window.
- **Daily memories are scattered.** They sit in many windows, so no theme could
  span them.

Morii decided that ChatGPT, which already distills the daily memories, also
submits themes, and that Memory adds what is needed to hold sources from several
windows.

## Decision

- **Connector domain.** `SegmentedAutomationVault::open_capture_window_in(owner,
  days, sensitivity, forms, (access, egress))` opens a window in a given domain;
  `open_capture_window` keeps random domains. The connector derives one stable
  access and egress pair from the Vault ID and its account scope
  (`ConnectorWriter::domain`). It opens and reuses only windows in that domain,
  so all of its daily memories share one domain. No connector windows exist yet,
  so nothing is migrated.
- **Synthesis rule.** For a theme synthesis only (`kind: theme`, `change:
  synthesis`), the contract and the Vault's parent closure accept parents formed
  under other policies of the same access and egress domain. Everything else
  stays as it was:
  - every parent source must be among the theme's input refs, and readable
    under the theme's own policy;
  - subject, project, sensitivity and deletion are checked;
  - readers still need every parent to see a theme.

  Nothing outside the theme's own policy enters it.
- **Theme submission.** `ConnectorWriter::submit_theme(theme, preflight, now)`
  takes `ConnectorTheme { request_id, analyst_model, title (≤60), summary
  (≤2000), members (2–32 current memory IDs), note }`.
  - **Members.** Every member must be a current memory of the connector's
    domain and subject, with no project, readable at the connector's
    sensitivity ceiling.
  - **Synthesis policy.** A synthesis policy is created with an ID derived from
    the request. It allows only `theme`, lists exactly the members' sources, is
    in the connector domain and is valid for one day.
  - **Theme memory.** One theme memory is formed under it: interpretation
    `inference`, the summary as content, the title, each member's first quote
    as evidence, the union of their sources, and the members as `derived_from`.
    It is attributed to ChatGPT's model with prompt
    `chatgpt-connector-theme-1`.
  - **Preflight and replay.** Preflight writes nothing. A resubmitted request
    finds its theme by the synthesis policy and replays without writing.
- **Remote tool.** The remote write tool set gains `memory_submit_theme`
  (arguments `theme`, `preflight`), listed and callable under the same
  `memory.write` and initialized-session conditions as
  `memory_submit_conversation`.

## Consequences

- Themes over daily memories appear in the catalog, the theme families and
  Runtime's lanes, as ChatGPT's inference with links to the memories they cite.
  Morii can rename, pin or dissolve them with the existing owner actions.
- **Policy budget.** Each theme uses one policy of the Vault's 1024-policy
  budget; windows use the rest.
- **Coverage.** Themes cannot include initialization-era memories, which sit in
  other domains.
- **Contract.** The parent-policy rule is relaxed only for theme synthesis
  inside one domain.
- No persisted format, IPC command or wire change beyond the new remote tool.

## Verification

Two connector writers share an account scope but have different forms, so they
open two windows in one domain; a third writer has another scope.

- **Refusals.** A theme with one member, or with a member from the other domain,
  is refused.
- **Preflight** writes nothing.
- **Formation.** The theme over one memory from each window is formed as a theme
  synthesis: inference, two parents, the union of their two sources, two
  evidence quotes, the summary and the title.
- **Replay.** A resubmission replays the same theme without writing.
- **Remote session.** The session lists `memory_submit_theme`, and bad theme
  arguments return a tool error.
