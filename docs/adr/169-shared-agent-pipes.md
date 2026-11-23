# ADR-MEM-169 — Shared pipe names for parallel Agent sessions

Date: 2026-11-23. Status: Accepted (D1b follow-up); synthetic validation.

## Context

Runtime hosts one fixed pipe name per registered Agent (D1b, ADR-MEM-165/166).
`Pipe::listen` creates the name's only instance with
`FILE_FLAG_FIRST_PIPE_INSTANCE`, so one Agent can hold one connection at a time.

Claude Code and Codex start a separate MCP bridge for every session. Morii often
runs several sessions at once, so from the second session on, the bridge failed
to connect. It also failed during the brief gap after a connection ended, before
Runtime recreated the instance.

## Decision

- **Shared names.** `Pipe::listen_shared(id)` creates one more instance of a name
  that may have up to `SHARED_PIPE_INSTANCES` (8) at once. Runtime keeps one
  listening instance per allowed concurrent session, and each connection is
  served as before (enrollment from `client_pid`, reader and writers, the
  ADR-MEM-166 `serve_connection`).
  - **Security.** Every instance keeps the current-user DACL and
    `PIPE_REJECT_REMOTE_CLIENTS`.
  - **No first-instance flag.** A shared name cannot use
    `FILE_FLAG_FIRST_PIPE_INSTANCE`. Trust does not rest on owning the name:
    the server enrolls the client process and the bridge enrolls the server
    process, and both enrollments require the same user and logon session (ADR-MEM-165).
  - **No mixing.** `listen` and `listen_shared` cannot share a name, because
    Windows requires one instance limit per name.
- **Busy wait.** `Pipe::connect` waits with `WaitNamedPipeW` up to five seconds
  while every instance is busy, then tries again. Any other error still fails at
  once.
- `Pipe::listen` is unchanged and still reserves its name exclusively.

## Consequences

- Runtime can let several sessions of the same Agent connect at once: Claude
  Code windows, Codex tabs. It switches its Agent listeners to `listen_shared`
  and keeps a small pool of instances.
- Each connection holds its own permits; locking still stops all of them
  through the stop flag.
- No persisted format, IPC command or wire change.

## Verification

- **Parallel clients.** Three shared instances serve three clients in one
  process. Each side reports the other's PID, and bytes written by each client
  come back over its own pair only.
- **Busy wait.** With the only instance taken, a second client waits until a new
  instance appears 300 ms later, then connects.
- **Exclusivity.** A sole `listen` still refuses another `listen` or
  `listen_shared` of its name, and a shared name refuses `listen`.
- **Regression.** Existing pipe tests pass with the waiting `connect`.
