# ADR-MEM-163 — Grant-backed Agent permits renew in place

Date: 2026-11-22. Status: Accepted (Runtime R4 question, Memory decision under
Morii's delegation); synthetic validation.

## Context

Agent permits last at most five minutes. The `enouia-memory-mcp` bridge connects
once and serves a whole Claude Code or Codex session on one pipe. Every
protocol's `serve_connection` checks the reader and writer before and after each
message, so the first check after five minutes failed. The host then
disconnected the pipe and the bridge exited, which ended the session's Memory
tools. Since ADR-MEM-141 and ADR-MEM-160, a long-lived Owner read grant (90 days
by default) is the Agent's real authority. The five-minute permit is only a
freshness window under it.

Two approaches were considered. In the first, the bridge reconnects after expiry.
That needs a new frame, a replay of MCP `initialize` on Claude Code's behalf, and
an answer for in-flight calls whose execution is unknown, and it loses the
recall context. In the second, chosen here, the host renews the permit in place.

## Decision

`SegmentedAgentPermit::renew(vault)` (with the same method on the note,
correction, supersede and forget permits, which wrap a read permit):

- does nothing for a permit not issued under an Owner read grant, so
  formation-window permits still end at their own `valid_until`;
- refuses a revoked permit, or one from another vault instance or layout;
- revokes the permit and refuses if the grant is no longer the exact current
  revision, inactive, not yet valid or expired;
- otherwise extends the monotonic and wall-clock end to
  `min(now + 5 minutes, grant valid_until)`. It never shortens the end.

The permit's scope hash, including its nonce, stays the same. Recall context and
history cursors bound to that hash survive a renewal.

The host renews before checking. The `QualifiedAgentReader` gate (connection
checks and every read) and the writers' `check_connection` call `renew` first.
Each protocol loop checks the connection after reading a message, before
handling it, so a connection idle for longer than five minutes resumes. The
serve loops, the bridge and every pipe protocol are unchanged.

Revocation keeps its old timing. Each check still compares the grant with the
current revision, so a new revision (revoke, renew or narrow) revokes a live
connection at the next check. An explicitly revoked permit, such as one revoked
after `OutcomeUnknown`, never renews.

## Consequences

- An Agent session stays connected as long as its grant is current and unexpired.
  When the grant expires or changes, the session ends exactly as before.
- Runtime moves the pin and needs no call-site change.
- No persisted format, wire or protocol change. The Runtime surface changes
  only by the new `renew` methods.

## Verification

- `agent_forget::grant_backed_connections_renew_past_five_minutes_until_the_grant_changes`:
  - a grant-backed forget requester issued for 120 seconds still checks out at
    200 and 480 seconds, while a window requester of the same lifetime fails;
  - a new grant revision revokes two live connections;
  - an explicitly revoked permit does not renew;
  - renewal stops at the grant's own end.
- `owner_read_grant::grant_backed_reader_renews_in_place_and_keeps_its_recall_context`:
  - after 200 seconds the host reader still returns its recall trace under the
    same scope hash;
  - a raw permit that nobody renews has expired.
- Existing grant, Agent note, correction, forget and read tests pass.
