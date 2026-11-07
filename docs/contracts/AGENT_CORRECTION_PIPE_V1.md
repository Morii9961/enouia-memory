# Agent correction-report pipe v1

2026-11-04. [ADR123](../adr/123-agent-correction-mcp-transport.md).

Protocol memory-agent-pipe-7, recordSchema=2,
layout=automation-segmented-owner-v1,
profile=memory-agent-initialized-corrections-1. Five initialized reads plus
memory_correction_report; wire capability memory-agent-correction-1 and native
correction capability memory-agent-correction-native-1. Pipe-5/6 meanings stay
unchanged. The two delegations bind the same Agent and exact leased Vault;
retained native OS enrollment requires MemoryRead + SourceRead.

The exact closed Hello binds descriptor, Vault ID, initialized read fingerprint
and reachable correction input/output schemas. Old profiles, layouts, altered
hashes or extra fields refuse before business dispatch. Existing protected
kernel-peer checks, bounded Call/Reply framing, unique JSON keys and enrollment
lifetime/revocation apply. Both native delegations are checked before and after
each frame and before business calls; every exit revokes both and disconnects
the actual borrowed pipe.

Known results follow AGENT_CORRECTION_MCP_V1. Native unknown publication emits
no Reply. The Client sees OutcomeUnknown, retains the complete original parsed
MCP Value and refuses further sends. Never automatically resend, reconnect or
change a domain key. Only explicit recovery and fresh native setup allow the
exact original request/key to recover its independent report_saved receipt.

Explicit launcher attach mode automation-corrections-v1 takes only the existing
pipe, expected Host PID and Vault ID. It opens no Vault or enrollment and edits
no MCP configuration. Stdio error.data.originalRequest and output-failure
retention use the complete MCP Value; old profiles keep their own behavior.
Original formatting/bytes are not promised. There is no real model/Provider,
private-data access, Runtime pin or production activation in this transport.
