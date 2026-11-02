# MCP-Resume-1: native Agent enrollment and runnable local transport

2026-10-26 (Asia/Shanghai). Branch `codex/mcp-native-agent`, based on fetched
main `c9bf1be7819dba19915adf6031b1900c032b73d1`; rechecked unchanged before
delivery. [ADR 092](../adr/092-native-agent-local-transport.md) and the
[frozen contract / reader-writer matrix](../contracts/MCP_NATIVE_AGENT_V1.md)
define this slice. ADR87/88/89 references use their complete filenames.

## Delivered

A trusted native operator/launcher fixes the live process, Agent principal and
nonempty distinct scope ceiling before any wire input. Existing Windows SID,
terminal-session and retained process-object checks are reused; the kernel pipe
peer PID must match before handshake and each message. The protected pipe rejects
remote clients and reserves its first instance. This authenticates the enrolled
process, not its executable code or a human; compromise of the trusted native
launcher is outside that boundary. Command-line/native configuration must be
supplied by that trusted operator, never copied from an Agent request.

`mcp::native::serve_connection` borrows the already bound HostService and its
single writer. An opaque gate connection binds instance/generation and TTL;
Enrollment adds a five-minute monotonic maximum and shared native revocation.
The Agent endpoint cannot dispatch Workspace, Owner lifecycle or System calls.
Native checks also precede ping/list, not only business tools. Every Enrollment
is consumed once. The executable uses a ten-second frame/idle deadline; EOF,
invalid framing, expiry, lock/unlock, revocation and peer death terminate it.
Reconnection requires a new explicit native registration. No automatic renewal,
replay, resend or replacement key is implemented.

`enouia-memory-host` now has explicit `--agent-id` / `--agent-scopes` mode,
mutually exclusive with Owner `--surface`. `enouia-memory-mcp` is a runnable
native stdio launcher in the same crate: it starts one explicitly selected Host,
verifies that owned process, bridges bounded JSON-RPC and reaps only that child.
It has no Vault IO. On EOF it drops the pipe and gives the Host a bounded graceful
exit interval before forced owned-child cleanup. Starting against a root already
leased by another Host refuses; it never bypasses that Host or becomes a second
writer. Existing owner endpoint and default cargo run target stay unchanged.
Concurrent owner/Agent executable listeners and durable enrollment UI are not
implemented; native embedders can attach the borrowed-service entry point.

Five independent scope ceilings advertise exactly the existing six tools:
search/read/source, pending Agent proposal, limited current/offline Context and
provisional checkpoint. Current persisted resource/project/source/sensitivity/
delete policies are still enforced by the original HostPort. Agent speech does
not become Morii self-report or Owner confirmation. The old proposal/checkpoint
semantics do not add a mandatory review queue to automated V2 memory.

Client errors distinguish known closed/not-sent from ambiguous exchange. The
latter retains the unchanged original parsed MCP message, IPC requestId and key;
failed stdout also retains it in the native client. The stdio error returns
originalRequest where output remains writable. A failed channel cannot send again.
An explicit fresh registration may inspect the original legal durable receipt;
unknown results never fabricate success or trigger automatic recovery.

## Format and capability support

| Surface | schema-1 / host-current-v2 | schema-2 / automation-host-v2 |
| --- | --- | --- |
| Six MCP / Memory IPC v1 tools | Existing policy-filtered reader and pending/provisional writer semantics | Refused before business record decode/query/count; Agent reader/writer not delivered |
| Native Agent pipe / stdio | Exact memory-agent-pipe-1, memory-agent-six-tools-1, MCP 2025-11-25 and compatibility hash | Root, schema, layout, capability or hash mismatch refused, with no downgrade or empty-success fallback |
| Workspace / legacy Owner | Existing schemaVersion 1, 58 commands and original Owner history | No implicit new-format adoption or migration |
| AutomationHost native pipeline | No legacy rewriting | Existing native System-gated transactions and automatic staging/reassessment; not reachable by this Agent endpoint |
| Core-7 memory-native-read-1 | No legacy-owner projection | Existing System-only native read; never forwarded to Agent |

Git pin, record schema, segmented format 2, Host publication layout, Workspace
IPC, Memory IPC/MCP and native capabilities remain separate axes. The automated
V2 Agent reader is the next independent adaptation, not a completed exit here.

## Actual checks and observed limits

| Check | Result |
| --- | --- |
| Focused native process/pipe tests | Ten support cases passed, including child-process helper: current policy and deletion, independent source scope, sensitivity/count suppression, forged principal/scope/PID, six-tool ceiling, lock/unlock/revoke/expiry, wrong kernel peer, wrong root/schema/layout/capability/hash, duplicate fields and lifecycle frames, dead enrollment, descriptor-first V2 refusal, real post-commit reply loss, exact-key replay/conflict, Context/checkpoint Agent provenance and committed-output failure |
| Native monotonic expiry / shared-grant drop | Included in the full Host library checks |
| Runnable stdio / separate executable Host | Passed; only MCP JSON on stdout, six-tool catalog, actual Host search and sole-writer release after EOF |
| `cargo fmt --all -- --check` | Passed |
| `cargo test --workspace --locked` | Passed: 673 tests, zero failures, RUST_TEST_THREADS=1; initial failures are recorded below |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed, pinned Rust 1.98.1 / offline |
| Independent Schema cross-check | Passed using existing Python 3.12.10 / python-jsonschema 4.26.0: 50 schemas and all fixture groups; no installation or schema changes |
| Runtime surface generation/check | Passed; schemaVersion 1 / 58 commands retained; exact aggregate logged in RUNTIME.md |

The initial four-thread full run encountered `Busy` at the unchanged automation
job-status unwrap (`automatic_loop.rs:589`). Its exact isolated test passed;
the final serial full run is reported separately above. An intermediate serial
run exposed a test-driver issue: libtest prefixes the first child event with its
test name when single-threaded. The event parser was corrected and its ten
Agent cases then passed with RUST_TEST_THREADS=1. No adjacent automation logic
was changed to hide that observation. Restricted-token fixture ACL checks failed
before Vault creation; actual checks use the normal Windows identity rather than
weakening ACL requirements. Local ignored logs are under
`target/mcp-native-agent-validation/` (fmt, full runs, exact Busy recheck,
strict clippy and independent schema output).

All roots were newly created temporary synthetic directories. All process peers
were this test's own child/current processes. OS account/session metadata is used
only for authentication; no API credentials are read. No private Vault/history,
real Provider/embedding, real MCP application configuration, Runtime/Claude shell,
Activity/Usage input, migration, remote exposure or deployment was touched.

## Repeatable synthetic launch

From this repository, with pinned Rust and normal Windows ACL rights:

```powershell
$env:CARGO_NET_OFFLINE = 'true'
cargo test -p enouia-memory-host --lib native_agent --locked -- --test-threads=1
cargo test -p enouia-memory-host --lib native_enrollment --locked
cargo test -p enouia-memory-host --test transport native_agent_stdio_launcher_runs_six_tools_and_releases_the_only_writer_on_eof --locked -- --exact
```

These commands create their own roots, policies, sources and owned process peers,
then remove only their own synthetic roots. The stdio test exercises the real
`enouia-memory-mcp` binary, not the old backend-unavailable stdio example.

For an independently provisioned **synthetic** host-current-v2 root only, the
trusted native operator can run the binary with explicit native configuration:

```powershell
cargo build -p enouia-memory-host --bins --locked
& .\target\debug\enouia-memory-mcp.exe `
  --host-executable .\target\debug\enouia-memory-host.exe `
  --vault $syntheticRoot --vault-id $syntheticVaultId `
  --host-id $syntheticHostId --agent-id $syntheticAgentId `
  --agent-scopes 'memory:read,source:read,memory:propose,context:read,session:propose'
```

The variables must describe that newly created synthetic root and native grant;
there is no default root, bootstrap, discovery or automatic policy creation.
Send newline-delimited initialize, initialized notification and tools/list as
in the executable test. Per-record policy remains independently required; scope
ceiling alone grants no records. This is a startup recipe, not real-client config
or evidence that a personal library is ready.

## Ownership and stop

Exact owned-file allowlist (all other files, AGENTS and manual status snapshots
are excluded):

- `crates/enouia-memory-host/Cargo.toml`
- `crates/enouia-memory-host/src/main.rs`
- `crates/enouia-memory-host/src/bin/enouia-memory-mcp.rs`
- `crates/enouia-memory-host/src/mcp/mod.rs`
- `crates/enouia-memory-host/src/mcp/native.rs`
- `crates/enouia-memory-host/src/mcp/read.rs`
- `crates/enouia-memory-host/src/service.rs`
- `crates/enouia-memory-host/src/transport.rs`
- `crates/enouia-memory-host/tests/support/scoped_reads.rs`
- `crates/enouia-memory-host/tests/support/native_agent.rs`
- `crates/enouia-memory-host/tests/transport.rs`
- `docs/adr/README.md`
- `docs/adr/092-native-agent-local-transport.md`
- `docs/contracts/MCP_NATIVE_AGENT_V1.md`
- `docs/design/INTERFACES.md`
- `docs/handoff/MCP_RESUMPTION_2026-10-25.md`
- `docs/integration/RUNTIME.md`
- `docs/integration/runtime-surface.json`
- `docs/validation/MCP-native-agent-transport.md`

Only this feature branch is committed/pushed. No merge/deployment/automation or
next slice starts. Remaining exits: V2 versioned Agent reader with its own Agent
policy contract; real application registration/configuration and owner acceptance;
broader Context and Runtime native Host adoption. Daily memory remains fully
automatic, with optional human correction rather than obligatory per-item review.
