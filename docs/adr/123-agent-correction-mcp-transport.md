# ADR-MEM-123 — Explicit Agent correction MCP and local transport

2026-11-04. MCP-4 continuation of ADR118–122.

## Decision

Add an opt-in correction server with the five initialized read tools and
`memory_correction_report`. Native construction pairs an initialized reader and
an independently delegated AgentCorrectionReporter for the same authenticated
Agent and the exact same leased Vault instance. Their scopes remain independent;
neither read, note nor MemoryPropose authority grants correction permission.
The server calls the existing ADR121 intake, never NativeMemoryOwner.

Use a distinct `memory-agent-pipe-7`, profile
`memory-agent-initialized-corrections-1`, and wire capability
`memory-agent-correction-1`. Pipe-5 remains read-only and pipe-6 remains notes.
The exact closed hello binds this descriptor, the selected Vault, initialized
read compatibility and the reachable correction input/output schemas. Native
OS enrollment retains the real Agent peer and requires MemoryRead + SourceRead;
it pairs identity and never substitutes for the correction permit.

Check both delegations before each frame and call. Known refusals are bounded,
content-free errors with retryable=false. Success returns only the original
`report_saved` receipt: not correction, acceptance, queueing or Owner approval.
Submission still owns current target/source/policy and receipt-replay checks.

On native OutcomeUnknown emit no MCP tool result and no pipe Reply. Retain the
complete original parsed MCP Value, close the session, revoke both delegations
and actually disconnect the borrowed pipe. The existing poisoned Client retains
that Value and blocks sends. Explicit recovery and fresh native setup may replay
only the original domain request/key; never auto-resend, reconnect or replace it.
The stdio bridge preserves the full Value in originalRequest and on stdout
failure. This preserves parsed values, not original formatting or byte order.

The launcher adds explicit `--protocol automation-corrections-v1` attach mode.
It discovers no Vault, grants no permission and changes no real MCP configuration.
Runtime-only capture, optional manual control, automatic System consumption,
chain format and old-binary refusal remain unchanged. Supersede, forget,
forget-hide, real models/egress, Runtime adoption and MCP-5 are separate.

See [MCP contract](../contracts/AGENT_CORRECTION_MCP_V1.md) and
[pipe contract](../contracts/AGENT_CORRECTION_PIPE_V1.md).
