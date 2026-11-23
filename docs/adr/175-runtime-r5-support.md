# ADR-MEM-175 — Grant purpose labels, recall targets and brief lock waits

Date: 2026-11-24. Status: Accepted (Runtime R5 requests); synthetic validation.

## Context

Runtime R5 asked for three things that its own ports could not provide:

- **Connector grant identity.** Settings shows coverage and follow status for
  every registered Agent. For the ChatGPT connector it cannot, because only
  the remote service's file knows which grant the connector reads under.
- **Recall targets.** Saving a recall preview as a capsule (ADR-MEM-173) needs
  exact `TargetRef`s. `trace.selected` carries only memory ID and revision, so
  Runtime read every member's detail to get its hash.
- **Busy on parallel sessions.** Two sessions of one Agent connecting at once
  (ADR-MEM-169) race for the writer lock in read-only and permit-issuing paths.
  These tried once (`Duration::ZERO`), so the loser got `Busy`. Runtime worked
  around this with its own two-second retry.

Runtime also asked for turn times in `native_conversation_detail`. Each turn
already carries its `source_record` with `occurred_at`, `captured_at`,
`original_time`, `original_timezone` and `time_precision`, so no change was
needed.

## Decision

- **Purpose label.** `OwnerReadGrant` gains optional `purpose`: lowercase ASCII
  letters, digits and `_`, 1–64 characters, for example `chatgpt_connector`.
  It grants nothing. It only lets Runtime find a grant in `owner_read_status`.
  When absent, a grant's bytes and hash are unchanged. Morii sets it in the
  connector grant's JSON (or in a revision), and it goes through the usual
  plan and confirmation.
- **Recall targets.** `recall_preview_native` adds `selected_targets`, the
  exact current `TargetRef` of each `trace.selected` entry, in the same order.
- **Brief lock wait.** These paths now wait up to `BRIEF_LOCK_WAIT` (two
  seconds) for the writer lock:
  - Agent reads (`read_agent`);
  - permit issuance (`authorize_*` through `authorize_agent_profile`, and the
    grant-backed writer permits);
  - Agent note drafts;
  - the read-only Owner grant queries (`owner_read_grant`,
    `owner_read_status`, `owner_read_grant_coverage`,
    `plan_owner_read_grant`).

  Commits still try once, so an unknown outcome is never hidden behind a wait.
  Connection checks never take the writer lock.

## Consequences

- Runtime can show the connector's coverage and follow status by finding the
  grant whose `purpose` is `chatgpt_connector`, and it can drop the
  member-detail reads and its own Busy retry.
- Older binaries refuse a grant carrying `purpose`, as with the other optional
  grant fields. Move the pin before writing one.
- No IPC command or wire change. The grant schema adds one optional string.

## Verification

- **Purpose.** An empty label, one with a space or a hyphen, and one of 65
  characters are refused at planning. `chatgpt_connector` commits and is found
  in the Owner status.
- **Lock wait.** While another thread holds the writer lock inside an Agent
  read for 300 ms, an Owner status query waits and succeeds.
- **Recall targets.** The recall preview's `selected_targets` match
  `trace.selected` one to one, each with a full hash.
