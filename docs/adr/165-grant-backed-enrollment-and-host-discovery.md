# ADR-MEM-165 — Grant-backed native enrollment and bridge host discovery

Date: 2026-11-22. Status: Accepted (D1 follow-up, Runtime R4 request); synthetic validation.

## Context

ADR-MEM-163 lets a permit issued under an Owner read grant renew in place, so the
vault side of a long Agent connection no longer ends after five minutes. The
native `Enrollment` that admits the pipe client still carried its own
five-minute monotonic deadline: `live()` failed after it and `timeout()` cut every
read at it, so a long or idle Agent session still disconnected at five minutes.

Separately, the stdio bridge `enouia-memory-mcp` required `--host-pid` to enroll
the serving Host. Claude Code and Codex start MCP servers from static
configuration files, while the Runtime Host gets a new PID on every start, so the
PID cannot live in that configuration.

## Decision

**Enrollment follows the grant.** `Enrollment` gains a one-way flag. The serve
loops of the qualified, initialized, opening, correction, supersede, forget-request
and note pipes call `follow_grant(reader.grant_backed())` right after the reader's
(and writer's) connection check succeeds, before each read. A successful check has
just renewed a grant-backed permit against the exact current grant revision. Once
the flag is set:

- `live()` no longer applies the five-minute deadline. It still requires the
  enrolled process (held by handle, so the PID cannot be reused) to be alive, and
  the transport grant to be unrevoked. `check` still verifies the pipe client PID.
- `timeout()` returns the caller's timeout instead of cutting it at the deadline.

The connection then lives exactly as long as the grant-backed permit, which is
checked before and after every read. A grant change, revocation or expiry fails
that check and ends the connection at once (ADR-MEM-163). Window permits, the v1
Agent pipe and the catalog and v2 pipes keep the five-minute deadline unchanged.

**Bridge host discovery.** `--host-pid` becomes optional. Without it the bridge
connects to the pipe first, reads the serving process ID from the connected
client end (`GetNamedPipeServerProcessId`, new `Pipe::server_pid`), and enrolls it
with `ProcessPeer::enroll`. That requires the same user SID and the same logon
session and holds the process handle, as before. Every exchange still verifies the
server PID. With `--host-pid` the bridge checks the given PID as before.

The bridge does not check the Host's executable path or signature. A same-user
process can already read and change Runtime's files and memory, so such a check
would not stop it and would add upgrade-path maintenance. A Host PID file would
not stop it either and adds stale state. Pipe creation keeps
`FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS` and the current-user
DACL, so another user cannot squat the endpoint.

## Consequences

- Runtime can keep a stable pipe name per vault, principal and protocol, and
  write the bridge command into static MCP configuration without a PID.
- Only one instance of a pipe name exists at a time; Runtime recreates it after
  each connection and must queue or name concurrent clients separately.
- No wire, protocol, command or persisted-format change.

## Verification

- Unit test: an enrollment past its deadline fails while it follows a window
  permit. After `follow_grant(true)` it is live and its timeout is uncut. Revoking
  the transport grant ends it.
- The qualified stdio bridge test now starts the bridge without `--host-pid`. It
  attaches, serves and reaps on EOF. The other bridge tests keep `--host-pid`.
- Existing pipe and ADR-MEM-163 renewal tests pass.
- A real-time session longer than five minutes is not run in tests. It is left for
  Runtime integration.
