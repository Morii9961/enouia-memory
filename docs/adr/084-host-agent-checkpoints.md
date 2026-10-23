# ADR-MEM-84 — Agent provisional checkpoints

Date: 2026-10-23. Status: accepted within explicitly authorized MV-8.2 local synthetic development.

Bind session_checkpoint to the existing Host writer and session domain. Admit
only a separately authenticated native Agent with SessionPropose; current
persisted policy must authorize the session, every selected event and every
current/cited source dependency. Session/source records have no project field;
client arguments cannot supply one or replace their actual sensitivity.

Select existing events belonging to the requested session and branch within the
explicit inclusive from/to sequence range. Both endpoints must exist on that
branch. Never silently cover the full branch, inherited parent events or a
different session. Every decision/open-loop event ID must belong to the selected
coverage. The stored artifact records exact event IDs and their coverage hash.

A native domain entry accepts an Agent-only logical checkpoint input without
generated item IDs. Its stable key/payload binds session, branch, range, summary
and proposed items before IDs/time are allocated. Replays return the original
checkpoint revision, commit and operation receipt even after unrelated branch
events are appended. Changed range/items/body conflict. Existing owner
checkpoint API, receipt namespace and whole-branch behavior remain unchanged.

Checkpoint artifacts stay provisional and generated_by names the Agent. No
original user/session event, Canonical memory, review, approval or Provider
dispatch is created or modified. A later reviewed/stale/deleted checkpoint
cannot be returned as currently provisional. Requests cannot adopt owner
identity, grant themselves policies or select lifecycle controls.

Admission/final audits are mandatory and content-free. Witness/storage or
post-publication audit/freshness uncertainty closes admission and retains the
exact original packet/key without retry. A fresh explicit native admission may
recover the original receipt under current policy/deletion checks. Incomplete
Host publication remains fenced; MCP never repairs it.

This closes only the synthetic in-process checkpoint binding. Native external
Agent authentication/transport, real-client compatibility, broader Context,
installation and personal acceptance remain separate. No cloud, private data,
Runtime UI, migration, merge or deployment is authorized here.
