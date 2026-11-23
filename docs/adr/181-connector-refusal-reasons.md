# ADR-MEM-181 — Refusal reasons for ChatGPT submissions

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

A connector submission or theme that failed any self-check came back as a bare
`invalid_submission`. ChatGPT could not tell which of a dozen rules it broke, so
it could only guess, or give up. Examples of those rules: a quote must appear
verbatim in a user turn, there are length limits, and turn IDs must be unique.

A statement whose `kind` the service does not allow passed the self-check, then
failed later as a rejected formation job, also with no reason.

## Decision

- **Reasons.** `ConnectorSubmission::problems(allowed_forms)` and
  `ConnectorTheme::problems()` list every self-check the input fails, as short
  English sentences that name the field, and the statement index and message ID
  where relevant. They cover:
  - turn count, unique message IDs and turn text size;
  - a malformed request (one summary sentence);
  - `analyst_model`, statement count, title, quote, note and entity limits;
  - a disallowed `kind`, listing the allowed ones;
  - a statement's `message_id` with no matching turn, a quote from a
    non-user turn, and a quote that is not verbatim;
  - the theme's text limits, member count and repeated members.

  `valid()` now means "no problems".
- **Allowed forms.** These are checked at submission time, so a disallowed
  `kind` is refused before anything is captured.
- **Answer.** On a refusal, the remote tools return
  `{"error": "invalid_submission", "problems": [...]}`. A theme the Vault
  refuses after its self-check passes gets one fixed reason: every member must
  be a current memory this connector submitted, in its own domain.
- **Unchanged.** The Vault's own checks, other faults, the never-retry rule for
  unknown outcomes, and replay.

## Verification

- `remote_daily_process`, on both roots:
  - a submission whose quote is not verbatim and whose kind is not allowed
    returns both reasons;
  - a theme with a member outside the connector's domain returns the domain
    reason.
- The existing `connector_formation` and `remote_connector` tests pass.
