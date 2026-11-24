# ADR-MEM-187 — Late replays answer from what was recorded

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

ADR-MEM-185 left a known limit. When a connector request is resubmitted after
its capture window has ended, the replay finds the window but cannot get a
capture permit for it, so it was refused with `Denied`. The original capture
and its memories still stood, but ChatGPT saw an error for what should be a
harmless replay.

## Decision

When a resubmitted `request_id` finds its recorded window but that window is
inactive or has ended, `ConnectorWriter::submit` writes nothing and answers
from the record.

- **Turns.** Each turn is matched to its recorded source by conversation
  (segment rule applied, ADR-MEM-185), message ID and the window's source IDs.
- **Memories.** The current memories formed from those turns under that window
  (ADR-MEM-183).
- **Receipt.** `window_ended: true`, with no capture, queue or jobs.

Every other receipt has `window_ended: false`.

## Verification

In `connector_formation::a_conversation_continues_across_windows`, the first
request is resubmitted two days after its one-day window ended. The receipt
has `window_ended`, no capture, its one turn `m1` and its one memory, and the
Vault snapshot is unchanged.
