# ADR-MEM-158 — Remote MCP write tool for ChatGPT-distilled memories

Date: 2026-11-21. Status: Accepted (follows ADR-MEM-157); synthetic validation.

## Context

ADR-MEM-157 added the validation-and-storage core for memories ChatGPT distills,
but ChatGPT could not reach it. The remote service (ADR-MEM-129 to 133) served
only the read-only opening profile with `memory.read`. The 157 core also called
the Windows-only `RuntimeCaptureHost`, while the remote service is meant to run
on a Linux VPS as well. Its submission returned before formation, so ChatGPT
could not learn the outcome.

## Decision

- **Platform-neutral synchronous writer.** `connector_formation::ConnectorWriter`
  uses only Vault APIs. `submit(submission, preflight, now_ms)` works like this:
  - It finds the window that already holds a resubmitted request with the new
    Morii-only `capture_policy_for_request`, so a replay keeps its window even
    after that window has no unused IDs left.
  - Otherwise it picks an open capture window (ADR-MEM-156) with enough unused
    IDs, at least eight minutes left, and the configured sensitivity and forms,
    or opens a new one.
  - It takes a four-minute owner capture permit, captures, enqueues, and drives
    the captured formation step with the `SubmittedAnalyzer` until nothing is
    dispatchable (at most 64 steps).
  - It returns the capture, the queue receipt and content-free job statuses.
  - Preflight writes nothing. When no window exists it only reports
    `window_needed`.
  - Capture accepts only the `enouia-runtime` platform, so the connector's
    conversations are told apart by its configured `account_scope`.
- **Write tool.** `mcp::remote::connector::ConnectorSession` wraps a read
  session and adds `memory_submit_conversation`. Its arguments are
  `submission`, `preflight` and the 157 submission shape. Every other message
  goes to the read session unchanged. The tool is listed and callable only when:
  - the service enables the connector;
  - the bearer holds `memory.write`;
  - the session has finished `initialize` and `notifications/initialized`.

  Results come back as a tool result with `structuredContent`. Invalid input
  returns a tool error. An unknown or recovery-required outcome keeps the
  complete original message and halts the endpoint, as for reads. Nothing is
  retried.
- **Scopes.** `memory.write` exists in the protected-resource metadata, the
  authorization-server metadata and issued tokens only when the service file
  has a `connector` section. Morii's server-side approval of the client remains
  the authorization. Without the section the service is unchanged and
  read-only.
- **Service file.** The new optional `connector` section holds:
  - `producer_id`: the connector's Client principal;
  - `config`: `account_scope`, `default_timezone`, `max_sensitivity`,
    `allowed_forms` and `window_days` (1–90).

## Connector status for Runtime

Runtime reads the connector's authorization scope from
`NativeMemoryOwner::capture_windows()`: each open window's sensitivity ceiling,
forms, validity and unused IDs. Activity comes from the existing conversations
read, filtered by the connector's `account_scope`. ChatGPT-formed memories carry
the attribution from ADR-MEM-157. No new System read port is needed. The service
process state (running, OAuth clients) lives on the server and is not part of
the local Vault.

## Boundaries

No persisted format or IPC change. The remote MCP surface gains one tool and one
optional scope. Deploying the service and pointing ChatGPT at it need Morii's
separate decisions: where the Vault primary lives, a reachable HTTPS host,
whether his ChatGPT plan can call write tools, and the sensitivity ceiling.
Entities are still not formed.

## Verification

- Writer, on a synthetic Owner root:
  - mismatched quotes, unknown turns and a non-Client producer are refused;
  - preflight before any window writes nothing;
  - the first submission opens one window and forms one memory synchronously;
  - a turn with no statements ends `NoMemory` in the same window;
  - preflight with a window writes nothing;
  - resubmitting replays without forming twice;
  - the stored memory carries the model, prompt version and note.
- Remote session around a stub read session:
  - without `memory.write` the tool is neither listed nor called;
  - before initialization calls go to the read session;
  - after initialization the tool is listed;
  - invalid arguments are tool errors;
  - preflight reports `window_needed`;
  - a submission forms one memory.
- Metadata lists `memory.write` only when enabled.
- Existing remote HTTP, OAuth, service and capture tests still pass.
- Not built or run on Linux in this change.
