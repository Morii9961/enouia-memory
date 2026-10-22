# MV-8.1i · Native client reconnect boundary

Date: 2026-10-22 (Asia/Shanghai). Baseline
`905411ef1b0c8dd1932a411b8390670c08bdad76` (merged PR #46).
Continues owner-authorized MV-8; all data and processes are synthetic.

[ADR-MEM-78](../adr/078-native-client-reconnect-boundary.md) adds the opt-in
Windows `Broker`. It owns one enrolled native transport and no request queue.
Native queue tickets are private Arc identities, so they cannot wrap around,
come from page JSON, cross Broker instances or survive lifecycle changes.
The shell must capture a ticket before queuing work and inject its actual
native window surface. The adapter checks both enrolled and window scope,
validates the envelope and encoded-byte limit, and emits only Call frames for
ordinary page packets. Native control methods remain separate explicit calls.

The existing three-process drill now exercises the broker: an independently
enrolled broker connects to the actual Host executable, refuses old handshake
tickets and overlay writes/controls, sends a page lock, physically disconnects,
opens a new channel, explicitly unlocks, rejects the old queued work and replays
the original key. It checks the same receipt/commit/candidate, native lock and
unlock, normal Host exit and subsequent lease acquisition.

A separate real-service/native-pipe test drops the reply after `remember`
durably changes CURRENT. The client returns `UncertainCall`, drops the channel
and refuses queued/current requests while disconnected. The listener observes
no automatic reconnection. Only the test caller's explicit new channel,
handshake and original-key replay recover the candidate without another commit.
This fault-injection case runs the actual HostService in a thread; the normal
lock/reconnect drill uses separate processes. Neither is power-loss evidence.

Additional checks refuse foreign Broker tickets, malformed closed packets and
a schema-valid review envelope whose escaped JSON exceeds the transport cap.
An initial fixture used the wrong CandidateId prefix; its failure is preserved
in `.local/mv8-broker-focused.log`, and only the synthetic ID was corrected.

Validation:

- `cargo test -p enouia-memory-host --locked`: 27 tests passed, zero failures,
  including native kernel and process tests; doctests contained zero tests.
- Strict Host all-target Clippy passed, with its dependency closure checked.
- `cargo fmt --all -- --check`, the Python Runtime surface checker and the Rust
  Runtime-surface guard passed; manifest aggregate
  `e12900633f864e7c74919a0d4e957bf7bb0a1a4c329af8b6b263f5e68323c19b`.
- Public-content/diff review excludes concurrent user instructions and manual
  status-snapshot edits, which are preserved separately.

Logs/exit records are retained in ignored `.local/mv8-broker-*` files. Checks
use pinned Rust 1.98.1 GNU, offline/locked Cargo, four test threads, incremental
compilation disabled and dev/test debug information disabled. The workspace
suite is not rerun for this localized adapter addition; the unchanged storage,
schema and other domain code retain PR #46's 486-test and prior independent
43-schema evidence. This does not diagnose the historical default-parallel
ordered-barrier failure.

Ready is a local handshake observation, not a current health/session-expiry
guarantee. Errors after transport entry conservatively report an unknown call
outcome; the broker stores no retry payload or operation key. The caller must
preserve the original request and distinguish transport failure from a valid
Core error envelope. Scope, native enrollment and UI acceptance remain the
adopting shell's duties. This does not activate Tauri/Runtime, private Vaults,
Provider credentials/sends, MCP, multiple clients, maintenance or cloud work.
