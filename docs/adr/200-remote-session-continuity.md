# ADR-MEM-200 — MCP sessions continue across permit generations

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

The remote service renews its Vault read permit at least every four minutes
(ADR-MEM-133). Until now, each renewal ended every MCP session, and a
client's next request got 404. The MCP transport specification tells clients
to initialize again on 404, but a long ChatGPT chat would hit this every four
minutes. We cannot check how gracefully ChatGPT recovers before the live
hookup, and a client that surfaces the 404 makes the connector look broken
mid-chat.

## Decision

- At the end of a generation, the endpoint hands the next generation the IDs
  of its sessions, each with its bearer subject (`Endpoint::carry`):
  - live sessions come first;
  - IDs carried earlier but not yet resumed follow;
  - at most `MAX_CARRIED` (16) IDs are handed on.

  Nothing is carried after an unknown-outcome halt, because the service
  stops there.
- In the new generation (`Endpoint::resume`), a request naming a carried ID
  is checked against the request's verified bearer.
  - **Same subject.** A new session is opened under the new permit. It is
    initialized inside with a synthetic `initialize` and
    `notifications/initialized`, then serves the request. The client sees
    the same session continue.
  - **Other subject, or unknown ID.** 404, as before.
- The LRU limit of ADR-MEM-199 applies to resumed sessions as well.
- **Authority is unchanged.**
  - Every request still verifies its bearer token, so a revoked OAuth client
    gets 401.
  - Every generation still takes a fresh permit from the current grant, so a
    revoked or expired grant stops the service as before.
  - A resumed session reads only under the new permit.

## Verification

- `remote_http::issued_sessions_resume_in_the_next_generation_for_their_subject_only`:
  - **Carry.** A session from one endpoint is carried into the next.
  - **Refusals.** Another subject's token, and an ID never issued, both get
    404.
  - **Resume.** The owner's request is served. The new session sees
    `initialize`, `notifications/initialized` and the request, in that
    order.
  - **Next generation.** The resumed session is carried again.
- `remote_service` (real loopback service with a fake clock): past the
  renewal point, the old session ID answers `tools/list` with all six tools.
  A forged token still gets 401, and OAuth revocation still ends access with
  401.
