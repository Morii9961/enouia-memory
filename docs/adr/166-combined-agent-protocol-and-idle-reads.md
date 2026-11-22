# ADR-MEM-166 — Combined Agent protocol, idle reads and the bridge library

Date: 2026-11-22. Status: Accepted (D1 follow-up, Runtime R4 request); synthetic validation.

## Context

Morii's scope for Claude Code and Codex is: read, add notes, report corrections
and request forgetting. The pipe protocols each pair the read tools with one write
tool, and the bridge connects one protocol at a time. An Agent would therefore
need three MCP servers. Its read tools would appear three times under different
prefixes, and it would hold three enrollments and pipes.

Runtime found two more problems while hosting these pipes:

- **Read deadlines.** Each read has a deadline. A short one ends an idle Claude
  Code connection, and MCP clients do not restart a bridge that exited. A long
  one leaves a thread blocked in a read after Morii locks or quits. That thread
  holds the vault and its permits until the deadline.
- **Bridge distribution.** The bridge is a binary of `enouia-memory-host`.
  Runtime depends on the library through Git, so it cannot build that binary and
  must not commit a prebuilt one.

## Decision

- **Combined protocol `automation-agent-v1`** (`mcp::agent`).
  - **Shape.** One connection serves the initialized read tools plus
    `memory_note`, `memory_correction_report` and `memory_forget_request`.
  - **Writers.** `AgentWriters` holds the three writers, each optional.
    `tools/list` shows only the write tools whose writer is present. An absent
    tool's name falls through to the read side, which refuses it.
  - **Construction.** Every writer must belong to the reader's principal and
    vault, and every present writer's connection check must pass.
  - **Liveness.** The reader and every writer are checked before and after each
    call. An uncertain write outcome closes the connection as in the
    single-writer protocols.
  - **Shared code.** The write tools reuse each module's tool descriptor and call
    handler (`tool()` and `call()`), now shared with the single-writer servers,
    so behavior and wire stay identical.
  - **Hello.** The hello (`memory-agent-pipe-11`, capability
    `memory-agent-combined-1`) is fixed for every grant combination and hashes
    all three write schemas, so a bridge needs no grant knowledge.
  - **Scope.** The enrollment needs `source.read` whenever a writer is present.
- **Idle reads and stop.** `agent::native::serve_connection` takes
  `stop: &AtomicBool`.
  - **Idle wait.** Between frames it waits with no time limit on one pending
    overlapped read (`Pipe::read_idle`, `transport::read_idle`).
  - **Quarter-second checks.** Every quarter second it checks `stop`,
    `Enrollment::live`, and the reader's and writers' connection checks; the
    connection check also renews a grant-backed permit. If any check fails, the
    read is cancelled and drained and the connection ends.
  - **Pipe PID.** The pipe client PID cannot change on a connected pipe; it is
    verified after each frame arrives.
  - **Timeout.** The caller's `timeout` applies only to the rest of a started
    frame and to the reply.
  - **Unchanged protocols.** The seven single-writer and read-only
    `serve_connection` functions keep their signatures and behavior.
  - **Pipe layer.** `Pipe` operations now wait in quarter-second slices. Passing
    no deadline and a `proceed` callback is internal; existing deadlines behave
    as before.
- **Bridge library.** The bridge body moves to
  `mcp::bridge::run(args) -> Result<(), BridgeFailure>`, where `args` excludes the program
  name. The `enouia-memory-mcp` binary only calls it. Runtime ships its own
  `enouia-memory-mcp` binary that calls the same function at its pinned
  revision.

## Consequences

- Runtime gives Claude Code and Codex one MCP server per Agent:
  `enouia-memory-mcp --protocol automation-agent-v1 --pipe <id> --vault-id <vault>`.
- Locking or quitting sets `stop`; a serving thread then ends within about a
  quarter second and releases its permits and vault reference.
- A window-permit connection still ends at its own deadline, through the
  quarter-second `Enrollment::live` check.
- No persisted format or IPC command change. The new protocol name and hello are
  wire additions; the existing protocols are unchanged.

## Verification

- **In-process server.** No writer lists only the read tools. All three writers
  add exactly the three write tools in order, and one writer adds one tool. An
  ungranted write tool is refused. A writer of another principal fails
  construction.
- **Bridge round trip.** The bridge runs as `automation-agent-v1` without
  `--host-pid`. It initializes, lists tools, and saves a real note. It stays
  connected through 1.5 s of idle with a 500 ms timeout, answers a ping, and
  ends within 2 s of `stop`.
- **Regression.** Existing note, correction, forget, supersede, qualified,
  initialized, opening and catalog pipe tests pass after the shared-handler
  refactor and the sliced pipe waits.
