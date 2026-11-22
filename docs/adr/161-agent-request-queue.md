# ADR-MEM-161 — Morii's desktop queue for Agent correction and forget requests

Date: 2026-11-22. Status: Accepted (Morii's R4.3 item D defaults); synthetic validation.

## Context

Morii's default for local Agents (Claude Code, Codex): their correction requests
and forget requests take effect only after he confirms them on the desktop. The
Vault saves both kinds (ADR-MEM-121/125, and ADR-MEM-160 for grant-backed
Agents), but offered no Morii-facing way to list them. Saved correction reports
can also be consumed automatically by the captured worker (ADR-MEM-122), but only
when a host explicitly starts it with corrections. Neither the remote connector
(ADR-MEM-158) nor Runtime's registered entry does.

## Decision

- Add Morii-only `SegmentedAutomationVault::agent_requests(owner, max)` and
  `NativeMemoryOwner::agent_requests(max)`. Each returned item carries:
  - `kind` (`correction` / `forget`);
  - the request key, request ID, Agent principal and target revision;
  - the cited quoted user statement;
  - the Agent's untrusted text (proposed correction or forget reason);
  - the commit that saved it;
  - `pending`: the target is still that exact revision and not deleted.

  Requests whose cited source is above `max` or deleted are omitted. It is
  read-only, follows the owner lock, and returns no other content.
- **Acting** uses the existing Owner path:
  - for a correction: `plan(Correct { target, source_id, assertion_text })`,
    where Morii reviews and edits the text himself and the Agent's text is only
    a suggestion;
  - for a forget: `plan(Forget { target, .. })`;
  - then `confirm_from_owner_gesture` and `commit`.

  Correcting or forgetting the target changes its revision or deletes it, so
  the request stops being pending. Nothing is recorded about the request
  itself.
- **Dismissing** without acting is kept by Runtime locally (by request key). It
  is not recorded in the Vault, so no new delta or format is introduced.
- **No automatic consumption** in the paths Morii uses. The correction-consuming
  worker stays opt-in, and no Morii-facing host starts it. Hosts must keep it
  that way while the queue is the confirmation path.

## Addendum: hosting Agent pipes

Claude Code and Codex start the `enouia-memory-mcp` stdio bridge themselves, so
the hosting Runtime cannot know the bridge's PID before it connects.
`windows::Pipe::client_pid()` reads the connected client's PID from the OS on
the server end after `accept`. The host then calls `Enrollment::enroll(pid,
principal, scopes)` and `serve_connection`. The trust basis is unchanged:

- a per-Agent, per-protocol pipe name written only into that Agent's local MCP
  configuration;
- the current-user pipe ACL.

A PID reported by the bridge itself would add nothing, because only the OS value
can be checked.

## Consequences

- Dismissals are local to the desktop that made them and are not in backups.
  If they ever need to be durable, a later slice can add an Owner record.
- No persisted format, IPC or wire change.

## Verification

Synthetic Owner root with a saved forget request:

- the System actor is refused;
- Morii's queue shows one pending forget item with the cited target and the
  Agent's text;
- after Morii's ordinary Owner forget, the item is no longer pending;
- a locked owner is refused.

Existing forget, correction and owner tests pass.
