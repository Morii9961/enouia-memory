# ADR-MEM-184 — ChatGPT files correction and forget requests

Date: 2026-11-24. Status: Accepted (Morii 2026-11-24: "做，默认打开");
synthetic validation.

## Context

Local Agents can file correction and forget requests under a request-capable
read grant (ADR-MEM-160/161). Each request cites the user turn where Morii
asked for it, and Morii confirms it in Runtime. ChatGPT, where Morii actually
talks, had no such tools. When Morii said "that memory is wrong" or "forget
that one", nothing could reach his queue.

## Decision

- **Drafted grants.** `read-draft` sets `allow_requests` whenever the config
  allows source text (`source_read`), because requests need it. Morii chose
  this default.
- **Service.** When the connector is enabled and the configured grant is
  request-capable, each permit generation also creates the grant-backed
  correction reporter and forget requester for the same principal and scope.
  - Their connection checks join the serve loop's checks.
  - They are revoked with the reader.
  - An unknown outcome halts the service, as other writes do.
  - When the grant does not allow requests, the tools are not offered.
- **Receipts.** `memory_submit_conversation` receipts gain `turns`: each turn's
  `message_id` and captured source, in turn order. That is the source a request
  cites.
- **Tools.** Both take `request_id`, `memory_id`, `source_id` and `quote`, the
  quote being Morii's words in that turn, verbatim:
  - `memory_request_correction` also takes `correction`;
  - `memory_request_forget` also takes `reason`.

  The service resolves the memory's current target and the source's current
  revision. It then submits the existing `AgentCorrectionRequest` or
  `AgentForgetRequest`, unchanged, through the existing permits and checks.

  Free-form request keys map to stable request IDs (ADR-MEM-178), so a repeat
  replays. A quote the Vault cannot locate, or a citation the grant does not
  allow, returns `invalid_request` with a reason.
- **Unchanged.** Nothing changes until Morii confirms in Runtime. The requests
  appear in `agent_requests` like a local Agent's, with the connector's
  principal.

## Consequences

- Runtime shows these requests in the existing queue. Their principal is the
  connector's Agent ID: the principal of the grant whose `purpose` is
  `chatgpt_connector`. Runtime can label them "ChatGPT".
- A grant drafted before this change (`allow_requests` absent) offers no
  request tools until Morii confirms a revision that allows them.
- No schema, IPC or wire change. The Runtime surface records the remote service
  files.

## Verification

In `remote_daily_process`, on both roots:

- The drafted grant allows requests, and `tools/list` offers both tools.
- After the asking conversation is submitted, its receipt gives the turn's
  source.
- A correction request is saved, and repeating it returns the same report key.
- A forget request is saved.
- A quote Morii never said is refused.
- Searches still return the original memories.
- After reopen, `agent_requests` holds one pending correction and one pending
  forget request.
