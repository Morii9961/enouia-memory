# ADR-MEM-195 — ChatGPT turn times become UTC; argument errors say why

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The submit tool's schema invited an "RFC 3339 time of the turn". The Vault
accepts exactly one form, `YYYY-MM-DDTHH:MM:SS.mmmZ`. Common RFC 3339 times
were therefore refused, for example `2026-11-24T21:05:00+08:00` or a UTC time
without milliseconds. The refusal was `invalid_arguments` with no detail, the
same answer ChatGPT got for any extra field (`created_at`) or wrong enum.

## Decision

- **Times.** `chatgpt::utc_time` converts an RFC 3339 time to the Vault's
  form.
  - Accepted: `T` or `t`, optional fraction (truncated to milliseconds),
    and `Z`, `z` or a `±HH:MM` offset.
  - Refused: non-ASCII text, a missing offset, or a date that is not real.
  - In the short form, a turn's `occurred_at` that is not already in Vault
    form but converts is replaced by its UTC value. The text ChatGPT gave is
    kept as `original_time`, unless one was already supplied.
  - Times that do not convert are left as they are and refused with a
    reason.
- **Argument errors.** When the arguments of the conversation, theme,
  correction or forget tools do not parse, the reply is `invalid_arguments`
  with `problems: [<parser message>]`. The message names, for example, the
  unknown field and the expected ones. It goes only to the caller that sent
  the arguments, and the access log (ADR-MEM-189) still records only the
  error flag.
- The `occurred_at` description gives an offset example and says to leave it
  out when unknown.

## Verification

`remote_connector::chatgpt_turn_times_become_utc_and_bad_arguments_say_why`
covers the following:

- **Conversion.** Times with an offset, `Z`, lowercase letters,
  microseconds, a negative half-hour offset and a day rollover all convert.
  Times with a space separator, no offset, a short offset, an invalid month
  or full-width digits are refused.
- **Short form.** Both the conversion and `original_time` are kept as
  described above.
- **Unknown field.** A turn with an extra `created_at` is refused with
  `invalid_arguments`, and the problem names `created_at`.

`remote_daily_process` (real binary) captures a next-day turn sent with
`occurred_at: 2026-11-25T09:00:00+08:00`.
