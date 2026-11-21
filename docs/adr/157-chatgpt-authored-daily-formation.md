# ADR-MEM-157 — Daily memories distilled by ChatGPT, validated and stored by Memory

Date: 2026-11-21. Status: Accepted (Morii: "改成 ChatGPT 提炼"); synthetic validation.

## Context and direction change

Morii's real conversations happen in ChatGPT, which reaches Memory through the
remote MCP connector (ADR-MEM-129 to 133). Runtime R4.3 had planned server-side
formation from Runtime-captured turns with Memory's own analyzer and a model
Provider (items B and C). Morii decided that ChatGPT distills daily memories
inside the conversation itself and hands both the raw turns and its distillation
to Memory, which only validates and stores them. This replaces items B and C
(no Memory analyzer, Provider or Mock is needed). It also amends ADR-MEM-102,
which allowed only Runtime as a capture producer, and drops the earlier MCP
principle that MCP is not a second formation pipeline. The approach follows the
authored-analysis route of ADR-MEM-142.

## Decision

Add `automation::connector_formation`, independent of transport:

- **`ConnectorSubmission`** carries:
  - `request`: an ordinary `RuntimeCaptureRequest` with 1 to 8 turns;
  - `analyst_model`: the model ChatGPT reports, recorded as-is;
  - up to 32 `ConnectorStatement`s, each with `message_id`, `quote`, `title`,
    `interpretation`, `kind`, `event_time`, `valid_time` and an optional `note`.

  `valid()` requires every statement to quote, literally, a **user** turn of
  the same request.
- **`submit(host, analyzer, submission, preflight)`** works as follows:
  - Preflight runs the capture preflight and writes nothing.
  - Otherwise it registers the statements for each turn, captures the turns
    through the owner Runtime capture host, and enqueues them.
  - A replayed request returns the original capture and does not enqueue
    again. Capture idempotency is keyed by `request_id`, as before.
- **`SubmittedAnalyzer`** is a `CaptureAnalyzer` that returns exactly the
  statements submitted for a turn:
  - a turn submitted with no statements yields `NoMemory`;
  - a turn never registered in this process (for example after a restart)
    yields `Paused`, so it stays queued until ChatGPT resubmits the same
    request;
  - it never invents or sends anything.
- The **captured formation worker** forms the statements with all its existing
  checks:
  - the quote must resolve in the captured source text;
  - the policy must be active and cover the source, its sensitivity and the
    form;
  - deletion and forget barriers apply;
  - job keys keep formation exactly-once.

  Unknown outcomes keep their recovery rules: no resend and no new key.
- **Attribution.** Each memory records `prompt_version`
  `chatgpt-connector-authored-1` and `model_version` = `analyst_model`, both in
  the admission input and in its annotations, plus ChatGPT's note as the
  formation note. ADR-MEM-137 still applies: a title not contained in its quote
  is replaced by a span of the quote. ChatGPT should keep titles inside the
  quoted text if it wants them kept. Entities are not formed in this slice.

## Read ports for Runtime

- **Analyst and model:** catalog and detail items expose them as
  `record.producer_version` / `model_version` and the annotations'
  `prompt_version` / `formation_note`.
- **Conversation origin:** sources carry the capture provenance (`platform`,
  `account_scope`) and appear in the existing conversations reads. The
  connector uses its own platform and account scope values, distinct from the
  Runtime desktop.
- **Connector status and authorization scope:** no System read port exists
  yet. It belongs to the next slice, which wires the remote MCP write tool.

## Next slices and prerequisites

1. Wire the remote MCP write tool (`memory_submit_conversation`, preflight
   flag, a `memory.write` OAuth scope) into the remote service (ADR-MEM-133).
   The service enrolls itself as the capture producer, runs the worker with a
   `SubmittedAnalyzer`, and rotates capture windows (ADR-MEM-156).
2. Deployment needs Morii's separate authorization: a reachable HTTPS host for
   ChatGPT, OAuth setup, and where the Vault primary lives.

No persisted format, IPC command or wire change in this slice. The new module
sits inside the Runtime surface directory, so the manifest is regenerated;
Runtime adopts nothing for it.

## Verification

Synthetic Owner root, real owner capture host and captured worker:

- quotes absent from the user turn and unknown message IDs are refused;
- preflight writes nothing;
- a submission forms exactly one memory carrying the analyst model, prompt
  version and note;
- a turn without statements ends `NoMemory`;
- resubmitting replays the capture without enqueuing or forming twice.
