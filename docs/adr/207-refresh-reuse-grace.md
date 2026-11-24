# ADR-MEM-207 — A retried refresh token gets a two-minute grace

Date: 2026-11-24. Status: Accepted (route: ChatGPT daily use); synthetic
validation.

## Context

Refresh tokens rotate on every use. Replaying a rotated-away token revokes
the whole grant (ADR-MEM-132), the usual sign that a token was stolen.

ChatGPT's connector is reported to refresh the token before every tool call
(OpenAI developer community thread "ChatGPT MCP connector refreshes token on
every tool call", 2026). It can also make several tool calls at once. Two
ordinary cases would therefore present a rotated-away token:

- **Parallel calls.** Both refresh with the same token. The second request
  finds it already rotated.
- **Lost response.** A refresh response is lost on the tunnel, and ChatGPT
  retries with the token it still holds.

Each case revoked the grant. The connector then failed until Morii got a new
approval code over SSH. Other reports from the same forum say ChatGPT keeps
retrying the refresh after `invalid_grant` instead of asking to sign in
again, so the connector might also have to be removed and added again. With
one refresh per tool call, this would likely happen within the first days.

## Decision

A rotated-away refresh token gets fresh tokens, without revoking the grant,
when all of the following hold:

- it is presented within `REFRESH_REUSE_GRACE_MS` (two minutes) of its
  rotation;
- the client presenting it is the one it was issued to;
- its grant family still has a live refresh token, so it has not been
  revoked.

Tokens already issued stay valid, so parallel refreshes each get working
tokens.

Every other replay revokes the family as before:

- after the grace;
- from another client;
- of a consumed authorization code.

Rotation times are kept only for the grace. The state file stores them in an
optional `rotated` map, which older state files lack. An older binary
refuses a state file that still holds entries.

**Risk accepted.** Someone holding a stolen refresh token, together with the
client ID, can use it within two minutes of its rotation without revoking
the grant. The token is already a bearer secret valid for 30 days, so the
grace barely widens what a thief can do. It removes a likely daily lockout.

## Verification

`remote_oauth`:

- **Retry within the grace.** In
  `refresh_rotates_and_replays_revoke_the_whole_family`, the same client's
  retry gets new tokens. The earlier access token still verifies.
- **Replay after the grace.** It still revokes everything, the retry's
  tokens included.
- **Other client or revoked grant.**
  `refresh_reuse_grace_is_for_the_same_client_and_a_live_grant`: another
  client's replay revokes at once. After that, the owner's retry is refused
  too.

`remote_daily_process`, through the real binary, follows the reported
ChatGPT pattern for six turns:

- it refreshes before every call;
- it opens a new session each turn, so six sessions outnumber the four
  slots;
- in one turn, two refreshes use the same token.

Every refresh and every search succeeds. `remote_service` and `remote_http`
pass unchanged.
